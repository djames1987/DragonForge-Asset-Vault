use crate::{
    error::{AppError, AppResult},
    models::PackageFile,
    storage::Storage,
};
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug)]
pub struct ExtractedEntry {
    pub relative_path: String,
    pub original_filename: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub bytes: Vec<u8>,
    pub is_primary: bool,
}

#[derive(Debug)]
pub struct ExtractedPackage {
    pub primary_path: String,
    pub entries: Vec<ExtractedEntry>,
    pub referenced_dependencies: Vec<String>,
    pub missing_dependencies: Vec<String>,
}

#[derive(Debug)]
pub struct StoredPackage {
    pub primary_path: String,
    pub files: Vec<PackageFile>,
    pub referenced_dependencies: Vec<String>,
    pub missing_dependencies: Vec<String>,
}

pub async fn store_extracted_package(
    storage: &Storage,
    asset_id: &str,
    version_number: i64,
    package: ExtractedPackage,
) -> AppResult<StoredPackage> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut files = Vec::with_capacity(package.entries.len());

    for entry in package.entries {
        let (sha256, storage_path) = storage
            .store_bytes(&entry.bytes, entry.extension.as_deref())
            .await?;
        files.push(PackageFile {
            id: Uuid::new_v4().to_string(),
            asset_id: asset_id.to_string(),
            version_number,
            relative_path: entry.relative_path,
            original_filename: entry.original_filename,
            extension: entry.extension,
            mime_type: entry.mime_type,
            byte_size: entry.bytes.len() as i64,
            sha256,
            storage_path,
            is_primary: entry.is_primary,
            created_at: now.clone(),
        });
    }

    Ok(StoredPackage {
        primary_path: package.primary_path,
        files,
        referenced_dependencies: package.referenced_dependencies,
        missing_dependencies: package.missing_dependencies,
    })
}

pub async fn extract_zip(
    zip_path: PathBuf,
    requested_primary: Option<String>,
) -> AppResult<ExtractedPackage> {
    tokio::task::spawn_blocking(move || extract_zip_blocking(&zip_path, requested_primary))
        .await
        .map_err(|err| AppError::Other(anyhow::anyhow!("package extraction worker failed: {err}")))?
}

fn extract_zip_blocking(
    zip_path: &Path,
    requested_primary: Option<String>,
) -> AppResult<ExtractedPackage> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| AppError::BadRequest(format!("invalid ZIP archive: {err}")))?;

    let mut raw_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut item = archive
            .by_index(index)
            .map_err(|err| AppError::BadRequest(format!("invalid ZIP entry: {err}")))?;
        if item.is_dir() {
            continue;
        }

        let enclosed = item
            .enclosed_name()
            .ok_or_else(|| AppError::BadRequest("ZIP contains an unsafe path".to_string()))?;
        let relative = normalize_relative(&enclosed)?;
        if relative.is_empty() {
            continue;
        }

        let mut bytes = Vec::with_capacity(item.size().min(32 * 1024 * 1024) as usize);
        item.read_to_end(&mut bytes)?;
        if bytes.is_empty() {
            continue;
        }
        raw_entries.push((relative, bytes));
    }

    if raw_entries.is_empty() {
        return Err(AppError::BadRequest("ZIP package contains no files".to_string()));
    }

    let primary_path = if let Some(requested) = requested_primary {
        let normalized = normalize_relative(Path::new(&requested))?;
        if !raw_entries.iter().any(|(path, _)| path == &normalized) {
            return Err(AppError::BadRequest(format!(
                "primary file '{normalized}' was not found in the ZIP"
            )));
        }
        normalized
    } else {
        choose_primary(&raw_entries).ok_or_else(|| {
            AppError::BadRequest(
                "could not choose a primary file; specify one in the package import dialog"
                    .to_string(),
            )
        })?
    };

    let primary_bytes = raw_entries
        .iter()
        .find(|(path, _)| path == &primary_path)
        .map(|(_, bytes)| bytes.as_slice())
        .ok_or(AppError::NotFound)?;

    let available: HashSet<String> = raw_entries.iter().map(|(p, _)| p.clone()).collect();
    let mut referenced_dependencies = discover_dependencies(&primary_path, primary_bytes)
        .into_iter()
        .filter_map(|dependency| resolve_dependency_path(&primary_path, &dependency))
        .collect::<Vec<_>>();

    if Path::new(&primary_path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("obj"))
    {
        let mtls = referenced_dependencies
            .iter()
            .filter(|path| Path::new(path).extension().and_then(|v| v.to_str())
                .is_some_and(|v| v.eq_ignore_ascii_case("mtl")))
            .cloned()
            .collect::<Vec<_>>();
        for mtl_path in mtls {
            if let Some((_, mtl_bytes)) = raw_entries.iter().find(|(path, _)| path == &mtl_path) {
                for dependency in discover_dependencies(&mtl_path, mtl_bytes) {
                    if let Some(resolved) = resolve_dependency_path(&mtl_path, &dependency) {
                        referenced_dependencies.push(resolved);
                    }
                }
            }
        }
    }

    referenced_dependencies.sort();
    referenced_dependencies.dedup();
    let missing_dependencies = referenced_dependencies
        .iter()
        .filter(|dependency| !available.contains(*dependency))
        .cloned()
        .collect::<Vec<_>>();

    let entries = raw_entries
        .into_iter()
        .map(|(relative_path, bytes)| {
            let filename = Path::new(&relative_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&relative_path)
                .to_string();
            let extension = Path::new(&relative_path)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase());
            let mime_type = mime_for_extension(extension.as_deref()).map(str::to_string);
            let is_primary = relative_path == primary_path;
            ExtractedEntry {
                relative_path,
                original_filename: filename,
                extension,
                mime_type,
                bytes,
                is_primary,
            }
        })
        .collect();

    Ok(ExtractedPackage {
        primary_path,
        entries,
        referenced_dependencies,
        missing_dependencies,
    })
}

fn normalize_relative(path: &Path) -> AppResult<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                if !text.is_empty() {
                    parts.push(text.to_string());
                }
            }
            Component::CurDir => {}
            _ => {
                return Err(AppError::BadRequest(
                    "package path must be relative and may not contain '..'".to_string(),
                ))
            }
        }
    }
    Ok(parts.join("/"))
}

fn choose_primary(entries: &[(String, Vec<u8>)]) -> Option<String> {
    const PRIORITY: &[&str] = &["glb", "gltf", "obj", "fbx", "blend", "png", "jpg", "jpeg", "webp"];
    for extension in PRIORITY {
        if let Some((path, _)) = entries.iter().find(|(path, _)| {
            Path::new(path)
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        }) {
            return Some(path.clone());
        }
    }
    entries.first().map(|(path, _)| path.clone())
}

fn discover_dependencies(primary_path: &str, bytes: &[u8]) -> Vec<String> {
    let extension = Path::new(primary_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    match extension.as_str() {
        "obj" => {
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(value) = trimmed.strip_prefix("mtllib ") {
                    found.push(value.trim().replace('\\', "/"));
                }
            }
        }
        "mtl" => {
            for line in text.lines() {
                let trimmed = line.trim();
                for prefix in ["map_Kd ", "map_Ks ", "map_Bump ", "bump ", "norm ", "map_d "] {
                    if let Some(value) = trimmed.strip_prefix(prefix) {
                        found.push(value.split_whitespace().last().unwrap_or(value).replace('\\', "/"));
                    }
                }
            }
        }
        "gltf" => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                for key in ["buffers", "images"] {
                    if let Some(items) = json.get(key).and_then(|value| value.as_array()) {
                        for item in items {
                            if let Some(uri) = item.get("uri").and_then(|value| value.as_str()) {
                                if !uri.starts_with("data:") {
                                    found.push(uri.replace('\\', "/"));
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    found.sort();
    found.dedup();
    found
}

fn resolve_dependency_path(source_path: &str, dependency: &str) -> Option<String> {
    let dependency_path = Path::new(dependency);
    if dependency_path.is_absolute() {
        return None;
    }
    let base = Path::new(source_path).parent().unwrap_or_else(|| Path::new(""));
    normalize_relative(&base.join(dependency_path)).ok()
}

fn mime_for_extension(extension: Option<&str>) -> Option<&'static str> {
    match extension? {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gltf" => Some("model/gltf+json"),
        "glb" => Some("model/gltf-binary"),
        "obj" => Some("text/plain"),
        "mtl" => Some("text/plain"),
        "json" => Some("application/json"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        "mp3" => Some("audio/mpeg"),
        _ => Some("application/octet-stream"),
    }
}
