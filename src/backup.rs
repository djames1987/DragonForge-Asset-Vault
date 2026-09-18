use crate::{
    config::BackupConfig,
    error::{AppError, AppResult},
    models::{
        BackupCreateResponse, BackupFileEntry, BackupManifest, BackupStatusResponse, BackupSummary,
        BackupVerifyResponse,
    },
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};
use tokio::{fs, io::AsyncReadExt};
use tracing::{info, warn};

pub async fn create_backup(
    db: &SqlitePool,
    data_root: &Path,
    database_filename: &str,
    archive_root: Option<&Path>,
    config: &BackupConfig,
) -> AppResult<BackupCreateResponse> {
    let backup_root = absoluteish(&config.directory)?;
    let data_root = absoluteish(data_root)?;
    if backup_root.starts_with(&data_root) {
        return Err(AppError::BadRequest(
            "backup.directory must be outside the DragonForge data directory".to_string(),
        ));
    }

    fs::create_dir_all(&backup_root).await?;
    let backup_id = format!(
        "dragonforge-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
    );
    let final_dir = backup_root.join(&backup_id);
    let staging_dir = backup_root.join(format!(".{backup_id}.partial"));
    if fs::try_exists(&staging_dir).await? {
        fs::remove_dir_all(&staging_dir).await?;
    }
    fs::create_dir_all(&staging_dir).await?;

    let db_dir = staging_dir.join("database");
    fs::create_dir_all(&db_dir).await?;
    let db_snapshot = db_dir.join(database_filename);
    sqlite_snapshot(db, &db_snapshot).await?;

    let mut files = Vec::new();
    let mut asset_files = 0usize;
    let mut preview_files = 0usize;

    let db_entry = hash_file_entry(&staging_dir, &db_snapshot).await?;
    files.push(db_entry);

    let assets_src = data_root.join("assets");
    if fs::try_exists(&assets_src).await? {
        let copied = copy_tree_with_manifest(&assets_src, &staging_dir.join("assets"), &staging_dir).await?;
        asset_files = copied.len();
        files.extend(copied);
    }

    let previews_src = data_root.join("previews");
    if fs::try_exists(&previews_src).await? {
        let copied = copy_tree_with_manifest(
            &previews_src,
            &staging_dir.join("previews"),
            &staging_dir,
        )
        .await?;
        preview_files = copied.len();
        files.extend(copied);
    }

    if let Some(archive_root) = archive_root {
        let archive_assets = archive_root.join("assets");
        if fs::try_exists(&archive_assets).await? {
            let copied = copy_tree_with_manifest(
                &archive_assets,
                &staging_dir.join("archive").join("assets"),
                &staging_dir,
            )
            .await?;
            asset_files += copied.len();
            files.extend(copied);
        }
    }

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let total_bytes = files.iter().map(|entry| entry.byte_size).sum();
    let manifest = BackupManifest {
        format_version: 1,
        backup_id: backup_id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        dragonforge_version: env!("CARGO_PKG_VERSION").to_string(),
        database_file: format!("database/{database_filename}"),
        files,
        total_bytes,
        asset_files,
        preview_files,
    };

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|err| AppError::Other(anyhow::anyhow!("backup manifest serialization failed: {err}")))?;
    fs::write(staging_dir.join("manifest.json"), manifest_bytes).await?;
    fs::rename(&staging_dir, &final_dir).await?;

    let verification = verify_backup_dir(&final_dir).await?;
    if !verification.valid {
        return Err(AppError::Other(anyhow::anyhow!(
            "backup was created but failed verification"
        )));
    }

    let mut replicated_to = Vec::new();
    let mut replication_failures = Vec::new();
    for target in &config.replication_targets {
        match replicate_backup(&final_dir, target, &backup_id).await {
            Ok(path) => replicated_to.push(path.display().to_string()),
            Err(err) => {
                warn!(backup_id = %backup_id, target = %target.display(), error = %err, "backup replication failed");
                replication_failures.push(format!("{}: {}", target.display(), err));
            }
        }
    }

    prune_old_backups(&backup_root, config.keep.max(1)).await?;

    info!(
        backup_id = %backup_id,
        path = %final_dir.display(),
        files = manifest.files.len(),
        total_bytes = manifest.total_bytes,
        replicas = replicated_to.len(),
        "backup created and verified"
    );

    Ok(BackupCreateResponse {
        backup: BackupSummary {
            backup_id,
            created_at: manifest.created_at,
            path: final_dir.display().to_string(),
            total_bytes: manifest.total_bytes,
            files: manifest.files.len(),
            verified: true,
        },
        replicated_to,
        replication_failures,
    })
}

pub async fn status(config: &BackupConfig) -> AppResult<BackupStatusResponse> {
    let root = absoluteish(&config.directory)?;
    fs::create_dir_all(&root).await?;
    let backups = list_backups(&root).await?;
    Ok(BackupStatusResponse {
        backup_directory: root.display().to_string(),
        replication_targets: config
            .replication_targets
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        keep: config.keep,
        backups,
    })
}

pub async fn verify_named_backup(
    config: &BackupConfig,
    backup_id: &str,
) -> AppResult<BackupVerifyResponse> {
    validate_backup_id(backup_id)?;
    let root = absoluteish(&config.directory)?;
    let dir = root.join(backup_id);
    if !fs::try_exists(&dir).await? {
        return Err(AppError::NotFound);
    }
    verify_backup_dir(&dir).await
}

async fn list_backups(root: &Path) -> AppResult<Vec<BackupSummary>> {
    let mut summaries = Vec::new();
    let mut entries = fs::read_dir(root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        if !name.starts_with("dragonforge-") {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        let Ok(raw) = fs::read(&manifest_path).await else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<BackupManifest>(&raw) else {
            continue;
        };
        summaries.push(BackupSummary {
            backup_id: manifest.backup_id,
            created_at: manifest.created_at,
            path: path.display().to_string(),
            total_bytes: manifest.total_bytes,
            files: manifest.files.len(),
            verified: false,
        });
    }
    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(summaries)
}

async fn sqlite_snapshot(db: &SqlitePool, target: &Path) -> AppResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).await?;
    }
    let escaped = target
        .to_string_lossy()
        .replace('\'', "''")
        .replace('\\', "/");
    let sql = format!("VACUUM INTO '{escaped}'");
    sqlx::query(&sql).execute(db).await?;
    Ok(())
}

async fn copy_tree_with_manifest(
    source: &Path,
    destination: &Path,
    backup_root: &Path,
) -> AppResult<Vec<BackupFileEntry>> {
    let mut result = Vec::new();
    let mut queue = VecDeque::from([(source.to_path_buf(), destination.to_path_buf())]);

    while let Some((src_dir, dst_dir)) = queue.pop_front() {
        fs::create_dir_all(&dst_dir).await?;
        let mut entries = fs::read_dir(&src_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let src = entry.path();
            let dst = dst_dir.join(entry.file_name());
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                queue.push_back((src, dst));
            } else if file_type.is_file() {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent).await?;
                }
                fs::copy(&src, &dst).await?;
                result.push(hash_file_entry(backup_root, &dst).await?);
            }
        }
    }

    Ok(result)
}

async fn hash_file_entry(root: &Path, path: &Path) -> AppResult<BackupFileEntry> {
    let mut file = fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AppError::BadRequest("backup file escaped backup root".to_string()))?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(BackupFileEntry {
        relative_path: relative,
        byte_size: metadata.len(),
        sha256: hex::encode(hasher.finalize()),
    })
}

async fn verify_backup_dir(dir: &Path) -> AppResult<BackupVerifyResponse> {
    let raw = fs::read(dir.join("manifest.json")).await?;
    let manifest: BackupManifest = serde_json::from_slice(&raw)
        .map_err(|err| AppError::Other(anyhow::anyhow!("invalid backup manifest: {err}")))?;

    let mut missing_files = Vec::new();
    let mut corrupt_files = Vec::new();
    let mut checked_files = 0usize;

    for expected in &manifest.files {
        let path = safe_join(dir, &expected.relative_path)?;
        if !fs::try_exists(&path).await? {
            missing_files.push(expected.relative_path.clone());
            continue;
        }
        let actual = hash_file_entry(dir, &path).await?;
        checked_files += 1;
        if actual.byte_size != expected.byte_size || actual.sha256 != expected.sha256 {
            corrupt_files.push(expected.relative_path.clone());
        }
    }

    Ok(BackupVerifyResponse {
        backup_id: manifest.backup_id,
        valid: missing_files.is_empty() && corrupt_files.is_empty(),
        checked_files,
        missing_files,
        corrupt_files,
    })
}

async fn replicate_backup(source: &Path, target_root: &Path, backup_id: &str) -> AppResult<PathBuf> {
    let target_root = absoluteish(target_root)?;
    fs::create_dir_all(&target_root).await?;
    let target = target_root.join(backup_id);
    if fs::try_exists(&target).await? {
        fs::remove_dir_all(&target).await?;
    }
    let staging = target_root.join(format!(".{backup_id}.partial"));
    if fs::try_exists(&staging).await? {
        fs::remove_dir_all(&staging).await?;
    }
    copy_tree_raw(source, &staging).await?;
    let verify = verify_backup_dir(&staging).await?;
    if !verify.valid {
        let _ = fs::remove_dir_all(&staging).await;
        return Err(AppError::Other(anyhow::anyhow!("replicated backup failed verification")));
    }
    fs::rename(&staging, &target).await?;
    Ok(target)
}

async fn copy_tree_raw(source: &Path, destination: &Path) -> AppResult<()> {
    let mut queue = VecDeque::from([(source.to_path_buf(), destination.to_path_buf())]);
    while let Some((src_dir, dst_dir)) = queue.pop_front() {
        fs::create_dir_all(&dst_dir).await?;
        let mut entries = fs::read_dir(&src_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let src = entry.path();
            let dst = dst_dir.join(entry.file_name());
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                queue.push_back((src, dst));
            } else if file_type.is_file() {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent).await?;
                }
                fs::copy(&src, &dst).await?;
            }
        }
    }
    Ok(())
}

async fn prune_old_backups(root: &Path, keep: usize) -> AppResult<()> {
    let backups = list_backups(root).await?;
    for old in backups.into_iter().skip(keep) {
        let path = root.join(&old.backup_id);
        if fs::try_exists(&path).await? {
            fs::remove_dir_all(path).await?;
        }
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() || relative.contains("..") {
        return Err(AppError::BadRequest("invalid backup manifest path".to_string()));
    }
    Ok(root.join(path))
}

fn validate_backup_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || !value.starts_with("dragonforge-")
    {
        return Err(AppError::BadRequest("invalid backup id".to_string()));
    }
    Ok(())
}

fn absoluteish(path: &Path) -> AppResult<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_ids_reject_path_traversal() {
        assert!(validate_backup_id("../dragonforge-test").is_err());
        assert!(validate_backup_id("dragonforge-test/other").is_err());
        assert!(validate_backup_id("dragonforge-test").is_ok());
    }

    #[test]
    fn safe_join_rejects_parent_components() {
        let root = Path::new("/tmp/backup");
        assert!(safe_join(root, "../vault.db").is_err());
        assert!(safe_join(root, "assets/a.bin").is_ok());
    }
}
