use crate::{
    error::{AppError, AppResult},
    models::AssetRow,
    storage::Storage,
};
use std::path::{Path, PathBuf};

const MAX_THUMBNAIL_DIMENSION: u32 = 512;

pub fn is_previewable_image(extension: Option<&str>) -> bool {
    matches!(
        extension.map(|v| v.to_ascii_lowercase()).as_deref(),
        Some("jpg") | Some("jpeg") | Some("png") | Some("webp")
    )
}

pub async fn get_or_create_thumbnail(
    storage: &Storage,
    asset: &AssetRow,
) -> AppResult<Option<PathBuf>> {
    if !is_previewable_image(asset.extension.as_deref()) {
        return Ok(None);
    }

    let destination = storage.thumbnail_path(&asset.sha256);
    if tokio::fs::try_exists(&destination).await? {
        return Ok(Some(destination));
    }

    let source = storage.resolve_relative(&asset.storage_path)?;
    let destination_for_worker = destination.clone();
    let asset_id = asset.id.clone();

    tokio::task::spawn_blocking(move || {
        generate_thumbnail(&source, &destination_for_worker)
    })
    .await
    .map_err(|err| AppError::Other(anyhow::anyhow!("thumbnail worker failed: {err}")))??;

    tracing::info!(
        asset_id = %asset_id,
        thumbnail = %destination.display(),
        "thumbnail generated"
    );

    Ok(Some(destination))
}

fn generate_thumbnail(source: &Path, destination: &Path) -> AppResult<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let image = image::open(source)
        .map_err(|err| AppError::Other(anyhow::anyhow!("failed to decode image for thumbnail: {err}")))?;
    let thumbnail = image.thumbnail(MAX_THUMBNAIL_DIMENSION, MAX_THUMBNAIL_DIMENSION);
    thumbnail
        .save_with_format(destination, image::ImageFormat::Png)
        .map_err(|err| AppError::Other(anyhow::anyhow!("failed to save thumbnail: {err}")))?;
    Ok(())
}
