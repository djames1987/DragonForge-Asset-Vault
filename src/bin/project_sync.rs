use super::{Asset, DragonForgeClient, PackageManifest, Project, ProjectAsset};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnginePreset {
    pub id: String,
    pub display_name: String,
    pub export_subdir: String,
    pub project_markers: Vec<String>,
    pub import_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectExportPlan {
    pub project_id: String,
    pub engine: String,
    pub export_subdir: String,
    pub export_path: String,
    pub import_notes: String,
}

#[derive(Debug, Clone)]
pub struct ProjectSyncItem {
    pub asset_id: String,
    pub asset_name: String,
    pub pinned_version: i64,
    pub current_version: i64,
    pub missing_files: usize,
    pub modified_files: usize,
    pub outdated: bool,
    pub error: Option<String>,
}

impl ProjectSyncItem {
    pub fn status_label(&self) -> String {
        if let Some(error) = &self.error {
            return format!("ERROR: {error}");
        }
        let mut labels = Vec::new();
        if self.outdated {
            labels.push(format!("outdated (v{} → v{})", self.pinned_version, self.current_version));
        }
        if self.missing_files > 0 {
            labels.push(format!("{} missing", self.missing_files));
        }
        if self.modified_files > 0 {
            labels.push(format!("{} modified", self.modified_files));
        }
        if labels.is_empty() {
            "in sync".to_string()
        } else {
            labels.join(" · ")
        }
    }

    pub fn is_clean(&self) -> bool {
        !self.outdated && self.missing_files == 0 && self.modified_files == 0 && self.error.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct ProjectSyncReport {
    pub project_id: String,
    pub project_name: String,
    pub engine: String,
    pub export_path: String,
    pub marker_warnings: Vec<String>,
    pub items: Vec<ProjectSyncItem>,
}

impl ProjectSyncReport {
    pub fn in_sync_count(&self) -> usize {
        self.items.iter().filter(|item| item.is_clean()).count()
    }

    pub fn issue_count(&self) -> usize {
        self.items.len().saturating_sub(self.in_sync_count()) + self.marker_warnings.len()
    }
}

pub fn fetch_engine_presets(base: &str) -> Result<Vec<EnginePreset>, String> {
    DragonForgeClient::api_client()?
        .get(format!("{base}/api/engines/presets"))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

pub fn fetch_export_plan(base: &str, project: &Project) -> Result<ProjectExportPlan, String> {
    DragonForgeClient::api_client()?
        .get(format!("{base}/api/projects/{}/export-plan", project.id))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

pub fn add_current_asset(
    base: &str,
    project: &Project,
    asset: &Asset,
) -> Result<PathBuf, String> {
    let client = DragonForgeClient::api_client()?;
    let manifest: PackageManifest = client
        .get(format!("{base}/api/assets/{}/package", asset.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let existing_link = match client
        .get(format!("{base}/api/projects/{}/assets/{}", project.id, asset.id))
        .send()
        .map_err(|e| e.to_string())?
    {
        response if response.status().is_success() => Some(
            response.json::<ProjectAsset>().map_err(|e| e.to_string())?
        ),
        response if response.status() == StatusCode::NOT_FOUND => None,
        response => return Err(response.error_for_status().unwrap_err().to_string()),
    };

    let package_mode = is_package(&manifest);
    let (export_root, primary_target_override) = if let Some(link) = existing_link.as_ref() {
        let relative = link.relative_path.as_deref()
            .ok_or_else(|| "existing project asset link is missing its relative path".to_string())?;
        let primary_target = safe_existing_project_path(base, project, relative)?;
        let root = if package_mode {
            package_root_from_primary(&primary_target, &manifest.primary_path)?
        } else {
            primary_target.parent().unwrap_or(Path::new(&project.local_path)).to_path_buf()
        };
        (root, Some(primary_target))
    } else {
        let plan = fetch_export_plan(base, project)?;
        let root = PathBuf::from(&plan.export_path);
        let root = if package_mode {
            root.join(super::sanitize_component(&asset.name))
        } else {
            root
        };
        (root, None)
    };

    fs::create_dir_all(&export_root).map_err(|e| e.to_string())?;
    let mut primary_target = None;

    for file in &manifest.files {
        let target = if package_mode {
            export_root.join(Path::new(&file.relative_path))
        } else if file.is_primary {
            primary_target_override.clone()
                .unwrap_or_else(|| export_root.join(&file.original_filename))
        } else {
            export_root.join(&file.original_filename)
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        download_manifest_file(base, asset, &manifest, file, &target)?;
        if file.is_primary {
            primary_target = Some(target);
        }
    }

    let primary_target = primary_target.unwrap_or_else(|| export_root.clone());
    let relative_path = primary_target
        .strip_prefix(&project.local_path)
        .map_err(|_| "exported asset path escaped the project root".to_string())?
        .to_string_lossy()
        .replace('\\', "/");

    let body = serde_json::json!({
        "asset_id": asset.id,
        "relative_path": relative_path,
        "version_number": manifest.version_number
    });
    client
        .post(format!("{base}/api/projects/{}/assets", project.id))
        .json(&body)
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?;

    Ok(primary_target)
}

pub fn remove_asset(
    base: &str,
    project: &Project,
    asset: &Asset,
) -> Result<String, String> {
    let client = DragonForgeClient::api_client()?;
    let link = fetch_project_link(base, project, &asset.id)?;
    let manifest = fetch_manifest_version(base, asset, link.version_number)?;

    let relative = link.relative_path.as_deref()
        .ok_or_else(|| "project asset link does not contain an exported relative path".to_string())?;
    let primary_target = safe_existing_project_path(base, project, relative)?;
    let removal_target = if is_package(&manifest) {
        package_root_from_primary(&primary_target, &manifest.primary_path)?
    } else {
        primary_target
    };

    let project_root = PathBuf::from(&project.local_path);
    if !removal_target.starts_with(&project_root) || removal_target == project_root {
        return Err("refusing to remove a path outside the project root".to_string());
    }

    let existed = removal_target.exists();
    let staging = if existed {
        let parent = removal_target.parent()
            .ok_or_else(|| "exported asset path has no parent folder".to_string())?;
        let staged = parent.join(format!(
            ".dragonforge-remove-{}-{}",
            asset.id,
            chrono::Utc::now().timestamp_millis()
        ));
        fs::rename(&removal_target, &staged)
            .map_err(|e| format!("could not stage exported project copy for removal: {e}"))?;
        Some(staged)
    } else {
        None
    };

    let unlink = client
        .delete(format!("{base}/api/projects/{}/assets/{}", project.id, asset.id))
        .send()
        .map_err(|e| e.to_string())
        .and_then(|response| response.error_for_status().map_err(|e| e.to_string()));

    if let Err(err) = unlink {
        if let Some(staged) = &staging {
            let _ = fs::rename(staged, &removal_target);
        }
        return Err(format!("server unlink failed; project files were restored: {err}"));
    }

    if let Some(staged) = staging {
        if staged.is_dir() {
            fs::remove_dir_all(&staged).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&staged).map_err(|e| e.to_string())?;
        }
    }

    Ok(if existed {
        format!("Removed {} from {} and deleted {}.", asset.name, project.name, removal_target.display())
    } else {
        format!("Removed {} from {}. The exported project copy was already missing.", asset.name, project.name)
    })
}

pub fn check_project(
    base: &str,
    project: &Project,
    presets: &[EnginePreset],
) -> Result<ProjectSyncReport, String> {
    let client = DragonForgeClient::api_client()?;
    let assets: Vec<Asset> = client
        .get(format!("{base}/api/projects/{}/assets", project.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;
    let plan = fetch_export_plan(base, project)?;

    let marker_warnings = validate_engine_markers(project, presets);
    let mut items = Vec::with_capacity(assets.len());

    for asset in assets {
        let item = match inspect_asset(base, project, &asset) {
            Ok(item) => item,
            Err(err) => ProjectSyncItem {
                asset_id: asset.id.clone(),
                asset_name: asset.name.clone(),
                pinned_version: 0,
                current_version: asset.current_version,
                missing_files: 0,
                modified_files: 0,
                outdated: false,
                error: Some(err),
            },
        };
        items.push(item);
    }

    Ok(ProjectSyncReport {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        engine: project.engine.clone(),
        export_path: plan.export_path,
        marker_warnings,
        items,
    })
}

pub fn repair_project(
    base: &str,
    project: &Project,
    presets: &[EnginePreset],
) -> Result<ProjectSyncReport, String> {
    let client = DragonForgeClient::api_client()?;
    let assets: Vec<Asset> = client
        .get(format!("{base}/api/projects/{}/assets", project.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    for asset in &assets {
        let link = fetch_project_link(base, project, &asset.id)?;
        export_pinned_version(base, project, asset, &link)?;
    }

    check_project(base, project, presets)
}

pub fn update_project_to_latest(
    base: &str,
    project: &Project,
    presets: &[EnginePreset],
) -> Result<ProjectSyncReport, String> {
    let client = DragonForgeClient::api_client()?;
    let assets: Vec<Asset> = client
        .get(format!("{base}/api/projects/{}/assets", project.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    for asset in &assets {
        add_current_asset(base, project, asset)?;
    }

    check_project(base, project, presets)
}

fn inspect_asset(base: &str, project: &Project, asset: &Asset) -> Result<ProjectSyncItem, String> {
    let link = fetch_project_link(base, project, &asset.id)?;
    let manifest = fetch_manifest_version(base, asset, link.version_number)?;
    let targets = expected_targets(base, project, &link, &manifest)?;

    let mut missing_files = 0usize;
    let mut modified_files = 0usize;
    for (file, target) in manifest.files.iter().zip(targets.iter()) {
        if !target.exists() {
            missing_files += 1;
            continue;
        }
        let actual = sha256_file(target)?;
        if actual != file.sha256 {
            modified_files += 1;
        }
    }

    Ok(ProjectSyncItem {
        asset_id: asset.id.clone(),
        asset_name: asset.name.clone(),
        pinned_version: link.version_number,
        current_version: asset.current_version,
        missing_files,
        modified_files,
        outdated: asset.current_version != link.version_number,
        error: None,
    })
}

fn export_pinned_version(
    base: &str,
    project: &Project,
    asset: &Asset,
    link: &ProjectAsset,
) -> Result<(), String> {
    let manifest = fetch_manifest_version(base, asset, link.version_number)?;
    let targets = expected_targets(base, project, link, &manifest)?;
    for (file, target) in manifest.files.iter().zip(targets.iter()) {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        download_manifest_file(base, asset, &manifest, file, target)?;
    }
    Ok(())
}

fn expected_targets(
    base: &str,
    project: &Project,
    link: &ProjectAsset,
    manifest: &PackageManifest,
) -> Result<Vec<PathBuf>, String> {
    let relative = link.relative_path.as_deref()
        .ok_or_else(|| "project asset link is missing an exported relative path".to_string())?;
    let primary_target = safe_existing_project_path(base, project, relative)?;
    if is_package(manifest) {
        let root = package_root_from_primary(&primary_target, &manifest.primary_path)?;
        Ok(manifest.files.iter().map(|file| root.join(&file.relative_path)).collect())
    } else {
        Ok(manifest.files.iter().map(|file| {
            if file.is_primary {
                primary_target.clone()
            } else {
                primary_target.parent().unwrap_or(Path::new(&project.local_path)).join(&file.original_filename)
            }
        }).collect())
    }
}

fn fetch_project_link(base: &str, project: &Project, asset_id: &str) -> Result<ProjectAsset, String> {
    DragonForgeClient::api_client()?
        .get(format!("{base}/api/projects/{}/assets/{asset_id}", project.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())
}

fn fetch_manifest_version(base: &str, asset: &Asset, version: i64) -> Result<PackageManifest, String> {
    DragonForgeClient::api_client()?
        .get(format!("{base}/api/assets/{}/versions/{version}/package", asset.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())
}

fn download_manifest_file(
    base: &str,
    asset: &Asset,
    manifest: &PackageManifest,
    file: &super::PackageFile,
    target: &Path,
) -> Result<(), String> {
    let client = DragonForgeClient::api_client()?;
    let mut response = if file.id.starts_with("single-") {
        client.get(format!(
            "{base}/api/assets/{}/versions/{}/download",
            asset.id, manifest.version_number
        ))
    } else {
        client.get(format!(
            "{base}/api/assets/{}/versions/{}/package/files/{}/download",
            asset.id, manifest.version_number, file.id
        ))
    }
    .send().map_err(|e| e.to_string())?
    .error_for_status().map_err(|e| e.to_string())?;

    let temp = target.with_extension(format!(
        "{}dragonforge-part",
        target.extension().and_then(|value| value.to_str()).map(|value| format!("{value}.")).unwrap_or_default()
    ));
    let mut output = fs::File::create(&temp).map_err(|e| e.to_string())?;
    io::copy(&mut response, &mut output).map_err(|e| e.to_string())?;
    drop(output);

    let actual = sha256_file(&temp)?;
    if actual != file.sha256 {
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "downloaded file hash mismatch for {}",
            file.relative_path
        ));
    }

    if target.exists() {
        if target.is_dir() {
            return Err(format!("cannot overwrite directory {}", target.display()));
        }
        fs::remove_file(target).map_err(|e| e.to_string())?;
    }
    fs::rename(&temp, target).map_err(|e| e.to_string())?;
    Ok(())
}

fn safe_existing_project_path(
    base: &str,
    project: &Project,
    relative: &str,
) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() {
        return Err("project asset path must be relative".to_string());
    }

    let mut clean = PathBuf::new();
    for component in relative_path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("project asset path contains an unsafe component".to_string())
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err("project asset path is empty".to_string());
    }

    let project_root = PathBuf::from(&project.local_path);
    let target = project_root.join(&clean);
    if !target.starts_with(&project_root) {
        return Err("project asset path escaped the project root".to_string());
    }

    // New Phase 11 locations must remain under the engine export root.
    // Legacy DragonForgeAssets links are still accepted for projects created before Phase 11.
    let plan = fetch_export_plan(base, project)?;
    let engine_root = PathBuf::from(plan.export_path);
    let legacy_root = project_root.join("DragonForgeAssets");
    if !target.starts_with(&engine_root) && !target.starts_with(&legacy_root) {
        return Err(format!(
            "project asset path is outside both the engine export root ({}) and legacy DragonForgeAssets",
            engine_root.display()
        ));
    }
    Ok(target)
}

fn package_root_from_primary(primary_target: &Path, primary_path: &str) -> Result<PathBuf, String> {
    let components = Path::new(primary_path)
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count();
    if components == 0 {
        return Err("package primary path is invalid".to_string());
    }
    let mut root = primary_target.to_path_buf();
    for _ in 0..components {
        if !root.pop() {
            return Err("could not determine package export root".to_string());
        }
    }
    Ok(root)
}

fn is_package(manifest: &PackageManifest) -> bool {
    manifest.files.len() > 1 || !manifest.referenced_dependencies.is_empty()
}

fn validate_engine_markers(project: &Project, presets: &[EnginePreset]) -> Vec<String> {
    let Some(preset) = presets.iter().find(|preset| preset.id.eq_ignore_ascii_case(&project.engine)) else {
        return vec![format!("No engine preset loaded for {}", project.engine)];
    };
    if preset.project_markers.is_empty() {
        return Vec::new();
    }
    let root = PathBuf::from(&project.local_path);
    let found = preset.project_markers.iter().any(|marker| root.join(marker).exists());
    if found {
        Vec::new()
    } else {
        vec![format!(
            "{} project marker not detected. Expected one of: {}",
            preset.display_name,
            preset.project_markers.join(", ")
        )]
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_item_clean_only_when_no_drift() {
        let clean = ProjectSyncItem {
            asset_id: "a".into(),
            asset_name: "A".into(),
            pinned_version: 2,
            current_version: 2,
            missing_files: 0,
            modified_files: 0,
            outdated: false,
            error: None,
        };
        assert!(clean.is_clean());
    }
}
