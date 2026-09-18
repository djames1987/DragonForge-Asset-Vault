use crate::{
    db,
    error::{AppError, AppResult},
    models::{StorageTierMoveResponse, StorageTierStatusResponse},
    storage::Storage,
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncReadExt};
use tracing::{info, warn};

pub async fn status(
    db_pool: &SqlitePool,
    storage: &Storage,
    asset_id: &str,
) -> AppResult<StorageTierStatusResponse> {
    let objects = db::list_asset_storage_objects(db_pool, asset_id).await?;
    let (tier, transitioned_at) = db::get_asset_storage_tier(db_pool, asset_id).await?;
    Ok(StorageTierStatusResponse {
        asset_id: asset_id.to_string(),
        tier,
        archive_enabled: storage.archive_enabled(),
        object_count: objects.len(),
        transitioned_at,
    })
}

pub async fn archive_asset(
    db_pool: &SqlitePool,
    storage: &Storage,
    asset_id: &str,
) -> AppResult<StorageTierMoveResponse> {
    validate_archive_root(storage)?;
    let objects = db::list_asset_storage_objects(db_pool, asset_id).await?;
    let mut mappings = Vec::new();
    let mut bytes_moved = 0u64;

    for object in &objects {
        if object.storage_path.starts_with("archive://") {
            continue;
        }

        let source = storage.resolve_relative(&object.storage_path)?;
        if !fs::try_exists(&source).await? {
            return Err(AppError::NotFound);
        }

        let archive_key = storage.archive_key(&object.storage_path)?;
        let target = storage.resolve_relative(&archive_key)?;
        copy_verified(&source, &target, &object.sha256).await?;
        bytes_moved = bytes_moved.saturating_add(fs::metadata(&target).await?.len());
        mappings.push((object.storage_path.clone(), archive_key));
    }

    db::replace_asset_storage_paths(db_pool, asset_id, &mappings, "archive").await?;

    let mut source_copies_removed = 0usize;
    for (old, _) in &mappings {
        if db::count_storage_path_references(db_pool, old).await? == 0 {
            let path = storage.resolve_relative(old)?;
            if fs::try_exists(&path).await? {
                match fs::remove_file(&path).await {
                    Ok(()) => source_copies_removed += 1,
                    Err(err) => warn!(
                        asset_id = %asset_id,
                        path = %path.display(),
                        error = %err,
                        "archive succeeded but unused hot copy could not be removed"
                    ),
                }
            }
        }
    }

    info!(
        asset_id = %asset_id,
        objects_moved = mappings.len(),
        bytes_moved,
        source_copies_removed,
        "asset archived"
    );

    Ok(StorageTierMoveResponse {
        asset_id: asset_id.to_string(),
        tier: "archive".to_string(),
        objects_moved: mappings.len(),
        bytes_moved,
        source_copies_removed,
    })
}

pub async fn recall_asset(
    db_pool: &SqlitePool,
    storage: &Storage,
    asset_id: &str,
) -> AppResult<StorageTierMoveResponse> {
    let objects = db::list_asset_storage_objects(db_pool, asset_id).await?;
    let mut mappings = Vec::new();
    let mut bytes_moved = 0u64;

    for object in &objects {
        if !object.storage_path.starts_with("archive://") {
            continue;
        }

        let source = storage.resolve_relative(&object.storage_path)?;
        if !fs::try_exists(&source).await? {
            return Err(AppError::NotFound);
        }

        let hot_key = storage.hot_key(&object.storage_path)?;
        let target = storage.resolve_relative(&hot_key)?;
        copy_verified(&source, &target, &object.sha256).await?;
        bytes_moved = bytes_moved.saturating_add(fs::metadata(&target).await?.len());
        mappings.push((object.storage_path.clone(), hot_key));
    }

    db::replace_asset_storage_paths(db_pool, asset_id, &mappings, "hot").await?;

    let mut source_copies_removed = 0usize;
    for (old, _) in &mappings {
        if db::count_storage_path_references(db_pool, old).await? == 0 {
            let path = storage.resolve_relative(old)?;
            if fs::try_exists(&path).await? {
                match fs::remove_file(&path).await {
                    Ok(()) => source_copies_removed += 1,
                    Err(err) => warn!(
                        asset_id = %asset_id,
                        path = %path.display(),
                        error = %err,
                        "recall succeeded but unused archive copy could not be removed"
                    ),
                }
            }
        }
    }

    info!(
        asset_id = %asset_id,
        objects_moved = mappings.len(),
        bytes_moved,
        source_copies_removed,
        "asset recalled to hot storage"
    );

    Ok(StorageTierMoveResponse {
        asset_id: asset_id.to_string(),
        tier: "hot".to_string(),
        objects_moved: mappings.len(),
        bytes_moved,
        source_copies_removed,
    })
}

async fn copy_verified(source: &Path, target: &Path, expected_sha256: &str) -> AppResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).await?;
    }

    let temp = temporary_target(target);
    if fs::try_exists(&temp).await? {
        let _ = fs::remove_file(&temp).await;
    }

    fs::copy(source, &temp).await?;
    let actual = sha256_file(&temp).await?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = fs::remove_file(&temp).await;
        return Err(AppError::Other(anyhow::anyhow!(
            "storage tier copy verification failed for {}",
            source.display()
        )));
    }

    if fs::try_exists(target).await? {
        let existing = sha256_file(target).await?;
        if existing.eq_ignore_ascii_case(expected_sha256) {
            fs::remove_file(&temp).await?;
            return Ok(());
        }
        fs::remove_file(target).await?;
    }

    fs::rename(&temp, target).await?;
    Ok(())
}

async fn sha256_file(path: &Path) -> AppResult<String> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn temporary_target(target: &Path) -> PathBuf {
    let filename = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    target.with_file_name(format!(".{filename}.dragonforge-tier-partial"))
}

fn validate_archive_root(storage: &Storage) -> AppResult<()> {
    let archive = storage.archive_directory()?;
    let hot = absoluteish(storage.root())?;
    let archive = absoluteish(archive)?;
    if archive.starts_with(&hot) {
        return Err(AppError::BadRequest(
            "storage.archive_dir must be outside storage.data_dir".to_string(),
        ));
    }
    Ok(())
}

fn absoluteish(path: &Path) -> AppResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_partial_name_is_hidden_and_distinct() {
        let target = Path::new("/vault/assets/aa/file.glb");
        let temp = temporary_target(target);
        assert_ne!(temp, target);
        assert!(temp
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("dragonforge-tier-partial"));
    }
}
