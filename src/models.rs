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
    pub auth_enabled: bool,
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


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseStatus {
    Complete,
    Warning,
    Unknown,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseAssessment {
    pub status: LicenseStatus,
    pub license_id: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub attribution_required: bool,
    pub license_url: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLicenseEntry {
    pub asset_id: String,
    pub asset_name: String,
    pub version_number: i64,
    pub license_id: String,
    pub status: LicenseStatus,
    pub creator: Option<String>,
    pub source_url: Option<String>,
    pub attribution_required: bool,
    pub warnings: Vec<String>,
    pub credit_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLicenseReport {
    pub project_id: String,
    pub generated_at: String,
    pub entries: Vec<ProjectLicenseEntry>,
    pub warning_count: usize,
    pub unknown_count: usize,
    pub credits_text: String,
    pub csv_manifest: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticStatusResponse {
    pub enabled: bool,
    pub ollama_reachable: bool,
    pub model: String,
    pub indexed_assets: i64,
    pub total_active_assets: i64,
    pub stale_assets: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    pub asset: Asset,
    pub semantic_score: f32,
    pub keyword_score: f32,
    pub combined_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResponse {
    pub query: String,
    pub mode: String,
    pub model: String,
    pub indexed_assets: i64,
    pub results: Vec<SemanticSearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexResponse {
    pub requested: usize,
    pub indexed: usize,
    pub failed: usize,
    pub model: String,
    pub failures: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SemanticSearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SemanticEmbeddingRow {
    pub asset_id: String,
    pub model: String,
    pub document_hash: String,
    pub dimensions: i64,
    pub embedding_json: String,
    pub indexed_at: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFileEntry {
    pub relative_path: String,
    pub byte_size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub format_version: u32,
    pub backup_id: String,
    pub created_at: String,
    pub dragonforge_version: String,
    pub database_file: String,
    pub files: Vec<BackupFileEntry>,
    pub total_bytes: u64,
    pub asset_files: usize,
    pub preview_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSummary {
    pub backup_id: String,
    pub created_at: String,
    pub path: String,
    pub total_bytes: u64,
    pub files: usize,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCreateResponse {
    pub backup: BackupSummary,
    pub replicated_to: Vec<String>,
    pub replication_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupVerifyResponse {
    pub backup_id: String,
    pub valid: bool,
    pub checked_files: usize,
    pub missing_files: Vec<String>,
    pub corrupt_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatusResponse {
    pub backup_directory: String,
    pub replication_targets: Vec<String>,
    pub keep: usize,
    pub backups: Vec<BackupSummary>,
}


#[derive(Debug, Clone, Serialize)]
pub struct EnginePreset {
    pub id: &'static str,
    pub display_name: &'static str,
    pub export_subdir: &'static str,
    pub project_markers: &'static [&'static str],
    pub import_notes: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectExportPlan {
    pub project_id: String,
    pub engine: String,
    pub export_subdir: String,
    pub export_path: String,
    pub import_notes: String,
}


#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AssetCheckout {
    pub asset_id: String,
    pub holder: String,
    pub workstation: String,
    pub note: Option<String>,
    pub checked_out_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutRequest {
    pub holder: String,
    pub workstation: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutStatusResponse {
    pub asset_id: String,
    pub checkout: Option<AssetCheckout>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageObjectRef {
    pub storage_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageTierStatusResponse {
    pub asset_id: String,
    pub tier: String,
    pub archive_enabled: bool,
    pub object_count: usize,
    pub transitioned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageTierMoveResponse {
    pub asset_id: String,
    pub tier: String,
    pub objects_moved: usize,
    pub bytes_moved: u64,
    pub source_copies_removed: usize,
}


#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Administrator,
    Developer,
    ReadOnly,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Administrator => "administrator",
            Self::Developer => "developer",
            Self::ReadOnly => "read_only",
        }
    }

    pub fn can_write(&self) -> bool {
        matches!(self, Self::Administrator | Self::Developer)
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Administrator)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct VaultUser {
    pub id: String,
    pub username: String,
    pub role: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub role: UserRole,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub role: Option<UserRole>,
    pub enabled: Option<bool>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMeResponse {
    pub enabled: bool,
    pub authenticated: bool,
    pub username: Option<String>,
    pub role: Option<UserRole>,
}


#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditEvent {
    pub id: String,
    pub occurred_at: String,
    pub actor_user_id: Option<String>,
    pub actor_username: Option<String>,
    pub actor_role: Option<String>,
    pub workstation: Option<String>,
    pub action: String,
    pub method: String,
    pub path: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub result: String,
    pub status_code: i64,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuditQuery {
    pub username: Option<String>,
    pub action: Option<String>,
    pub result: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportResponse {
    pub generated_at: String,
    pub events: Vec<AuditEvent>,
    pub csv: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAssetBrowserEntry {
    pub asset: Asset,
    pub pinned_version: i64,
    pub relative_path: Option<String>,
    pub added_at: String,
    pub outdated: bool,
}


#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectAssetCount {
    pub project_id: String,
    pub asset_count: i64,
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetRelationshipKind {
    Variant,
    Derivative,
    Export,
    Lod,
    Collision,
    Texture,
    Material,
    Animation,
    EngineExport,
    Reference,
}

impl AssetRelationshipKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Variant => "variant",
            Self::Derivative => "derivative",
            Self::Export => "export",
            Self::Lod => "lod",
            Self::Collision => "collision",
            Self::Texture => "texture",
            Self::Material => "material",
            Self::Animation => "animation",
            Self::EngineExport => "engine_export",
            Self::Reference => "reference",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AssetRelationship {
    pub id: String,
    pub source_asset_id: String,
    pub related_asset_id: String,
    pub kind: String,
    pub label: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRelationshipEntry {
    pub relationship: AssetRelationship,
    pub direction: String,
    pub asset: Asset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAssetRelationshipRequest {
    pub related_asset_id: String,
    pub kind: AssetRelationshipKind,
    pub label: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAssetRelationshipRequest {
    pub kind: Option<AssetRelationshipKind>,
    pub label: Option<Option<String>>,
    pub note: Option<Option<String>>,
}


#[cfg(test)]
mod phase18_relationship_tests {
    use super::*;

    #[test]
    fn relationship_kinds_use_stable_snake_case_json() {
        let cases = [
            (AssetRelationshipKind::Variant, ""variant""),
            (AssetRelationshipKind::Derivative, ""derivative""),
            (AssetRelationshipKind::Export, ""export""),
            (AssetRelationshipKind::Lod, ""lod""),
            (AssetRelationshipKind::Collision, ""collision""),
            (AssetRelationshipKind::Texture, ""texture""),
            (AssetRelationshipKind::Material, ""material""),
            (AssetRelationshipKind::Animation, ""animation""),
            (AssetRelationshipKind::EngineExport, ""engine_export""),
            (AssetRelationshipKind::Reference, ""reference""),
        ];

        for (kind, expected) in cases {
            assert_eq!(serde_json::to_string(&kind).unwrap(), expected);
        }
    }

    #[test]
    fn relationship_kind_as_str_matches_json_contract() {
        assert_eq!(AssetRelationshipKind::Variant.as_str(), "variant");
        assert_eq!(AssetRelationshipKind::Derivative.as_str(), "derivative");
        assert_eq!(AssetRelationshipKind::Export.as_str(), "export");
        assert_eq!(AssetRelationshipKind::Lod.as_str(), "lod");
        assert_eq!(AssetRelationshipKind::Collision.as_str(), "collision");
        assert_eq!(AssetRelationshipKind::Texture.as_str(), "texture");
        assert_eq!(AssetRelationshipKind::Material.as_str(), "material");
        assert_eq!(AssetRelationshipKind::Animation.as_str(), "animation");
        assert_eq!(AssetRelationshipKind::EngineExport.as_str(), "engine_export");
        assert_eq!(AssetRelationshipKind::Reference.as_str(), "reference");
    }
}
