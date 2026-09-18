use crate::error::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
    assets_dir: PathBuf,
    temp_dir: PathBuf,
    previews_dir: PathBuf,
}

impl Storage {
    pub async fn new(root: PathBuf) -> anyhow::Result<Self> {
        let assets_dir = root.join("assets");
        let temp_dir = root.join("temp");
        let previews_dir = root.join("previews");
        fs::create_dir_all(&assets_dir).await?;
        fs::create_dir_all(&temp_dir).await?;
        fs::create_dir_all(&previews_dir).await?;
        Ok(Self {
            root,
            assets_dir,
            temp_dir,
            previews_dir,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn temp_path(&self) -> PathBuf {
        self.temp_dir.join(format!("{}.upload", Uuid::new_v4()))
    }

    pub fn final_path(&self, sha256: &str, extension: Option<&str>) -> PathBuf {
        let first = &sha256[0..2];
        let second = &sha256[2..4];
        let mut filename = sha256.to_string();
        if let Some(ext) = extension.filter(|v| !v.is_empty()) {
            filename.push('.');
            filename.push_str(ext);
        }
        self.assets_dir.join(first).join(second).join(filename)
    }

    pub fn thumbnail_path(&self, sha256: &str) -> PathBuf {
        let first = &sha256[0..2];
        let second = &sha256[2..4];
        self.previews_dir
            .join(first)
            .join(second)
            .join(format!("{sha256}.png"))
    }

    pub fn relative_path(&self, full_path: &Path) -> AppResult<String> {
        full_path
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .map_err(|_| {
                AppError::BadRequest(
                    "asset path is outside configured vault storage".to_string(),
                )
            })
    }

    pub fn resolve_relative(&self, relative: &str) -> AppResult<PathBuf> {
        if relative.contains("..") {
            return Err(AppError::BadRequest("invalid storage path".to_string()));
        }
        Ok(self.root.join(relative))
    }

    pub async fn commit_temp(
        &self,
        temp_path: &Path,
        sha256: &str,
        extension: Option<&str>,
    ) -> AppResult<PathBuf> {
        let final_path = self.final_path(sha256, extension);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        if fs::try_exists(&final_path).await? {
            let _ = fs::remove_file(temp_path).await;
            return Ok(final_path);
        }

        fs::rename(temp_path, &final_path).await?;
        Ok(final_path)
    }

    pub async fn store_bytes(
        &self,
        bytes: &[u8],
        extension: Option<&str>,
    ) -> AppResult<(String, String)> {
        let sha256 = hex::encode(Sha256::digest(bytes));
        let final_path = self.final_path(&sha256, extension);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        if !fs::try_exists(&final_path).await? {
            fs::write(&final_path, bytes).await?;
        }
        let relative = self.relative_path(&final_path)?;
        Ok((sha256, relative))
    }

    pub async fn remove_temp(&self, path: &Path) {
        let _ = fs::remove_file(path).await;
    }
}

pub struct IncomingFile {
    pub temp_path: PathBuf,
    pub original_filename: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
}

pub async fn stream_field_to_temp(
    storage: &Storage,
    mut field: axum::extract::multipart::Field<'_>,
) -> AppResult<IncomingFile> {
    let original_filename = field
        .file_name()
        .map(ToString::to_string)
        .unwrap_or_else(|| "asset.bin".to_string());
    let mime_type = field.content_type().map(ToString::to_string);
    let extension = Path::new(&original_filename)
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_lowercase());

    let temp_path = storage.temp_path();
    tracing::info!(
        filename = %original_filename,
        temp_path = %temp_path.display(),
        "upload stream started"
    );

    let mut output = fs::File::create(&temp_path).await?;
    let mut hasher = Sha256::new();
    let mut byte_size: i64 = 0;

    while let Some(chunk) = field.chunk().await? {
        byte_size = byte_size
            .checked_add(chunk.len() as i64)
            .ok_or_else(|| AppError::BadRequest("uploaded file is too large".to_string()))?;
        hasher.update(&chunk);
        output.write_all(&chunk).await?;
    }
    output.flush().await?;
    drop(output);

    if byte_size == 0 {
        storage.remove_temp(&temp_path).await;
        tracing::warn!(filename = %original_filename, "empty upload rejected");
        return Err(AppError::BadRequest("uploaded file is empty".to_string()));
    }

    let sha256 = hex::encode(hasher.finalize());
    tracing::info!(
        filename = %original_filename,
        byte_size,
        sha256 = %sha256,
        "upload stream completed"
    );

    Ok(IncomingFile {
        temp_path,
        original_filename,
        extension,
        mime_type,
        byte_size,
        sha256,
    })
}
