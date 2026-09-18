use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    #[serde(flatten)]
    pub row: AssetRow,
    pub tags: Vec<String>,
    pub current_version: i64,
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

#[derive(Debug, Serialize, Deserialize)]
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
    pub extension: Option<String>,
    #[serde(default)]
    pub include_deleted: bool,
    #[serde(default)]
    pub deleted_only: bool,
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
    pub projects: i64,
}

#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct RestoreResponse {
    pub id: String,
    pub restored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub local_path: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub engine: Option<String>,
    pub local_path: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub engine: Option<String>,
    pub local_path: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectAssetRequest {
    pub asset_id: String,
    pub relative_path: Option<String>,
    pub version_number: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectAsset {
    pub project_id: String,
    pub asset_id: String,
    pub relative_path: Option<String>,
    pub version_number: i64,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AssetVersion {
    pub id: String,
    pub asset_id: String,
    pub version_number: i64,
    pub original_filename: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
    pub storage_path: String,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreVersionRequest {
    pub note: Option<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PackageFile {
    pub id: String,
    pub asset_id: String,
    pub version_number: i64,
    pub relative_path: String,
    pub original_filename: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
    pub storage_path: String,
    pub is_primary: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub asset_id: String,
    pub version_number: i64,
    pub primary_path: String,
    pub files: Vec<PackageFile>,
    pub referenced_dependencies: Vec<String>,
    pub missing_dependencies: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageImportResponse {
    pub asset: Asset,
    pub manifest: PackageManifest,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageVersionResponse {
    pub asset: Asset,
    pub manifest: PackageManifest,
}
