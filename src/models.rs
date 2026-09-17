use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AssetRow {
    pub id: String,
    pub name: String,
    pub original_filename: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
    pub storage_path: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub creator: Option<String>,
    pub license: Option<String>,
    pub attribution_required: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    #[serde(flatten)]
    pub row: AssetRow,
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct UploadMetadata {
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub creator: Option<String>,
    pub license: Option<String>,
    pub attribution_required: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAssetRequest {
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub source_url: Option<String>,
    pub creator: Option<String>,
    pub license: Option<String>,
    pub attribution_required: Option<bool>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct AssetQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub tag: Option<String>,
    #[serde(default)]
    pub include_deleted: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub duplicate: bool,
    pub asset: Asset,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: &'static str,
    pub phase: u8,
    pub version: &'static str,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub active_assets: i64,
    pub deleted_assets: i64,
    pub total_bytes: i64,
    pub unique_tags: i64,
}

#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub id: String,
    pub deleted: bool,
}
