#[path = "../logging.rs"]
mod logging;

use eframe::egui;
use reqwest::blocking::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io,
    path::{Component, Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HealthResponse {
    ok: bool,
    service: String,
    phase: u8,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SemanticStatusResponse {
    enabled: bool,
    ollama_reachable: bool,
    model: String,
    indexed_assets: i64,
    total_active_assets: i64,
    stale_assets: i64,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SemanticSearchResult {
    asset: Asset,
    semantic_score: f32,
    keyword_score: f32,
    combined_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SemanticSearchResponse {
    query: String,
    mode: String,
    model: String,
    indexed_assets: i64,
    results: Vec<SemanticSearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReindexResponse {
    requested: usize,
    indexed: usize,
    failed: usize,
    model: String,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupSummary {
    backup_id: String,
    created_at: String,
    path: String,
    total_bytes: u64,
    files: usize,
    verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupCreateResponse {
    backup: BackupSummary,
    replicated_to: Vec<String>,
    replication_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupVerifyResponse {
    backup_id: String,
    valid: bool,
    checked_files: usize,
    missing_files: Vec<String>,
    corrupt_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupStatusResponse {
    backup_directory: String,
    replication_targets: Vec<String>,
    keep: usize,
    backups: Vec<BackupSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Asset {
    id: String,
    name: String,
    original_filename: String,
    extension: Option<String>,
    byte_size: i64,
    sha256: String,
    category: Option<String>,
    description: Option<String>,
    source_url: Option<String>,
    creator: Option<String>,
    license: Option<String>,
    attribution_required: bool,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    tags: Vec<String>,
    current_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AssetVersion {
    id: String,
    asset_id: String,
    version_number: i64,
    original_filename: String,
    extension: Option<String>,
    mime_type: Option<String>,
    byte_size: i64,
    sha256: String,
    storage_path: String,
    note: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadResponse {
    duplicate: bool,
    asset: Asset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageFile {
    id: String,
    asset_id: String,
    version_number: i64,
    relative_path: String,
    original_filename: String,
    extension: Option<String>,
    mime_type: Option<String>,
    byte_size: i64,
    sha256: String,
    storage_path: String,
    is_primary: bool,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageManifest {
    asset_id: String,
    version_number: i64,
    primary_path: String,
    files: Vec<PackageFile>,
    referenced_dependencies: Vec<String>,
    missing_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageImportResponse {
    asset: Asset,
    manifest: PackageManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageVersionResponse {
    asset: Asset,
    manifest: PackageManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Project {
    id: String,
    name: String,
    engine: String,
    local_path: String,
    description: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectAsset {
    project_id: String,
    asset_id: String,
    relative_path: Option<String>,
    version_number: i64,
    added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LicenseStatus {
    Complete,
    Warning,
    Unknown,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectLicenseEntry {
    asset_id: String,
    asset_name: String,
    version_number: i64,
    license_id: String,
    status: LicenseStatus,
    creator: Option<String>,
    source_url: Option<String>,
    attribution_required: bool,
    warnings: Vec<String>,
    credit_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectLicenseReport {
    project_id: String,
    generated_at: String,
    entries: Vec<ProjectLicenseEntry>,
    warning_count: usize,
    unknown_count: usize,
    credits_text: String,
    csv_manifest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClientSettings {
    server_url: String,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8080".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct UploadForm {
    file_path: Option<PathBuf>,
    name: String,
    category: String,
    tags: String,
    description: String,
    creator: String,
    source_url: String,
    license: String,
    attribution_required: bool,
}

#[derive(Debug, Clone, Default)]
struct PackageForm {
    file_path: Option<PathBuf>,
    primary_path: String,
    name: String,
    category: String,
    tags: String,
    description: String,
    creator: String,
    source_url: String,
    license: String,
    attribution_required: bool,
}

#[derive(Debug, Clone, Default)]
struct EditForm {
    asset_id: String,
    name: String,
    category: String,
    tags: String,
    description: String,
    creator: String,
    source_url: String,
    license: String,
    attribution_required: bool,
}

#[derive(Debug, Clone)]
struct ProjectForm {
    name: String,
    engine: String,
    local_path: String,
    description: String,
}

impl Default for ProjectForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            engine: "Generic".to_string(),
            local_path: String::new(),
            description: String::new(),
        }
    }
}

enum ClientEvent {
    Health(Result<HealthResponse, String>),
    SemanticStatus(Result<SemanticStatusResponse, String>),
    SemanticReindex(Result<ReindexResponse, String>),
    BackupStatus(Result<BackupStatusResponse, String>),
    BackupCreated(Result<BackupCreateResponse, String>),
    BackupVerified(Result<BackupVerifyResponse, String>),
    Assets(Result<(Vec<Asset>, Option<String>), String>),
    Projects(Result<Vec<Project>, String>),
    Upload(Result<UploadResponse, String>),
    Update(Result<Asset, String>),
    Delete {
        asset_id: String,
        result: Result<(), String>,
    },
    Restore {
        asset_id: String,
        result: Result<(), String>,
    },
    Download {
        asset_id: String,
        result: Result<PathBuf, String>,
    },
    Thumbnail {
        asset_id: String,
        result: Result<Vec<u8>, String>,
    },
    ProjectCreated(Result<Project, String>),
    ProjectAssetAdded {
        project_id: String,
        asset_id: String,
        result: Result<PathBuf, String>,
    },
    ProjectAssetRemoved {
        project_id: String,
        asset_id: String,
        result: Result<String, String>,
    },
    Versions {
        asset_id: String,
        result: Result<Vec<AssetVersion>, String>,
    },
    VersionChanged {
        asset_id: String,
        result: Result<Asset, String>,
    },
    PackageImported(Result<PackageImportResponse, String>),
    PackageManifestLoaded {
        asset_id: String,
        result: Result<PackageManifest, String>,
    },
    PackageVersionChanged {
        asset_id: String,
        result: Result<PackageVersionResponse, String>,
    },
    ProjectLicenseFiles {
        project_id: String,
        result: Result<(PathBuf, usize, usize), String>,
    },
}

struct DragonForgeClient {
    settings: ClientSettings,
    assets: Vec<Asset>,
    projects: Vec<Project>,
    selected_id: Option<String>,
    selected_project_id: Option<String>,
    search: String,
    category_filter: String,
    tag_filter: String,
    extension_filter: String,
    license_filter: String,
    search_mode: String,
    semantic_status: Option<SemanticStatusResponse>,
    backup_status: Option<BackupStatusResponse>,
    deleted_only: bool,
    status: String,
    health: Option<HealthResponse>,
    upload: UploadForm,
    edit: EditForm,
    project_form: ProjectForm,
    package_form: PackageForm,
    show_upload: bool,
    show_edit: bool,
    show_project_create: bool,
    show_remove_project_confirm: bool,
    pending_remove_asset: Option<Asset>,
    pending_remove_project: Option<Project>,
    show_package_import: bool,
    show_package_contents: bool,
    show_versions: bool,
    version_asset_id: Option<String>,
    versions: Vec<AssetVersion>,
    version_note: String,
    package_manifest: Option<PackageManifest>,
    busy_count: usize,
    thumbnails: HashMap<String, egui::TextureHandle>,
    thumbnail_pending: HashSet<String>,
    tx: Sender<ClientEvent>,
    rx: Receiver<ClientEvent>,
}

impl DragonForgeClient {
    fn new() -> Self {
        let settings = match load_settings() {
            Ok(settings) => {
                info!(server_url = %settings.server_url, "client settings loaded");
                settings
            }
            Err(err) => {
                warn!(error = %err, "failed to load client settings; using defaults");
                ClientSettings::default()
            }
        };

        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            settings,
            assets: Vec::new(),
            projects: Vec::new(),
            selected_id: None,
            selected_project_id: None,
            search: String::new(),
            category_filter: String::new(),
            tag_filter: String::new(),
            extension_filter: String::new(),
            license_filter: "All".to_string(),
            search_mode: "Smart".to_string(),
            semantic_status: None,
            backup_status: None,
            deleted_only: false,
            status: "Ready".to_string(),
            health: None,
            upload: UploadForm::default(),
            edit: EditForm::default(),
            project_form: ProjectForm::default(),
            package_form: PackageForm::default(),
            show_upload: false,
            show_edit: false,
            show_project_create: false,
            show_remove_project_confirm: false,
            pending_remove_asset: None,
            pending_remove_project: None,
            show_package_import: false,
            show_package_contents: false,
            show_versions: false,
            version_asset_id: None,
            versions: Vec::new(),
            version_note: String::new(),
            package_manifest: None,
            busy_count: 0,
            thumbnails: HashMap::new(),
            thumbnail_pending: HashSet::new(),
            tx,
            rx,
        };

        info!("DragonForge desktop client initialized");
        app.check_server();
        app.refresh_semantic_status();
        app.refresh_backup_status();
        app.refresh_assets();
        app.refresh_projects();
        app
    }

    fn api_client() -> Result<Client, String> {
        Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .build()
            .map_err(|e| e.to_string())
    }

    fn base_url(&self) -> String {
        self.settings.server_url.trim_end_matches('/').to_string()
    }

    fn check_server(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        self.status = "Connecting to vault...".to_string();
        info!(server_url = %base, "server connection check started");

        thread::spawn(move || {
            let result = (|| -> Result<HealthResponse, String> {
                let client = Self::api_client()?;
                client
                    .get(format!("{base}/api/health"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::Health(result));
        });
    }

    fn refresh_semantic_status(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        thread::spawn(move || {
            let result = (|| -> Result<SemanticStatusResponse, String> {
                Self::api_client()?
                    .get(format!("{base}/api/search/semantic/status"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::SemanticStatus(result));
        });
    }

    fn reindex_semantic_search(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        self.status = "Reindexing semantic search with Ollama...".to_string();
        info!("semantic reindex started");
        thread::spawn(move || {
            let result = (|| -> Result<ReindexResponse, String> {
                Self::api_client()?
                    .post(format!("{base}/api/search/semantic/reindex"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::SemanticReindex(result));
        });
    }

    fn refresh_backup_status(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        thread::spawn(move || {
            let result = (|| -> Result<BackupStatusResponse, String> {
                Self::api_client()?
                    .get(format!("{base}/api/backups"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::BackupStatus(result));
        });
    }

    fn create_vault_backup(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        self.status = "Creating verified vault backup...".to_string();
        info!("vault backup started");
        thread::spawn(move || {
            let result = (|| -> Result<BackupCreateResponse, String> {
                Self::api_client()?
                    .post(format!("{base}/api/backups"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::BackupCreated(result));
        });
    }

    fn verify_latest_backup(&mut self) {
        let Some(status) = self.backup_status.as_ref() else {
            self.status = "Load Backup Status first.".to_string();
            return;
        };
        let Some(latest) = status.backups.first() else {
            self.status = "No backups exist yet.".to_string();
            return;
        };
        let backup_id = latest.backup_id.clone();
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        self.status = format!("Verifying {backup_id}...");
        thread::spawn(move || {
            let result = (|| -> Result<BackupVerifyResponse, String> {
                Self::api_client()?
                    .post(format!("{base}/api/backups/{backup_id}/verify"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::BackupVerified(result));
        });
    }

    fn refresh_assets(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        let search = self.search.trim().to_string();
        let category = self.category_filter.trim().to_string();
        let tag = self.tag_filter.trim().to_string();
        let extension = self
            .extension_filter
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase();
        let license_filter = self.license_filter.clone();
        let search_mode = self.search_mode.clone();
        let deleted_only = self.deleted_only;
        self.busy_count += 1;
        self.status = if deleted_only {
            "Loading recycle bin...".to_string()
        } else if search_mode == "Smart" && !search.is_empty() {
            "Running smart search...".to_string()
        } else {
            "Loading assets...".to_string()
        };

        info!(
            server_url = %base,
            search = %search,
            search_mode = %search_mode,
            category = %category,
            tag = %tag,
            extension = %extension,
            license_filter = %license_filter,
            deleted_only,
            "asset refresh started"
        );

        thread::spawn(move || {
            let result = (|| -> Result<(Vec<Asset>, Option<String>), String> {
                let client = Self::api_client()?;
                let (mut assets, search_note) =
                    if search_mode == "Smart" && !search.is_empty() && !deleted_only {
                        let response: SemanticSearchResponse = client
                            .get(format!("{base}/api/search/semantic"))
                            .query(&[("q", search.as_str())])
                            .send()
                            .map_err(|e| e.to_string())?
                            .error_for_status()
                            .map_err(|e| e.to_string())?
                            .json()
                            .map_err(|e| e.to_string())?;
                        let note = Some(format!(
                            "{} search · {} indexed · model {}",
                            if response.mode == "hybrid" { "Smart" } else { "Keyword fallback" },
                            response.indexed_assets,
                            response.model
                        ));
                        (
                            response.results.into_iter().map(|result| result.asset).collect(),
                            note,
                        )
                    } else {
                        let mut request = client.get(format!("{base}/api/assets"));
                        if !search.is_empty() {
                            request = request.query(&[("q", search.as_str())]);
                        }
                        if !category.is_empty() {
                            request = request.query(&[("category", category.as_str())]);
                        }
                        if !tag.is_empty() {
                            request = request.query(&[("tag", tag.as_str())]);
                        }
                        if !extension.is_empty() {
                            request = request.query(&[("extension", extension.as_str())]);
                        }
                        if deleted_only {
                            request = request.query(&[("deleted_only", "true")]);
                        }
                        let assets: Vec<Asset> = request
                            .send()
                            .map_err(|e| e.to_string())?
                            .error_for_status()
                            .map_err(|e| e.to_string())?
                            .json()
                            .map_err(|e| e.to_string())?;
                        (assets, None)
                    };

                if search_mode == "Smart" && !search.is_empty() && !deleted_only {
                    if !category.is_empty() {
                        assets.retain(|asset| {
                            asset.category.as_deref().unwrap_or("").eq_ignore_ascii_case(&category)
                        });
                    }
                    if !tag.is_empty() {
                        assets.retain(|asset| {
                            asset.tags.iter().any(|value| value.eq_ignore_ascii_case(&tag))
                        });
                    }
                    if !extension.is_empty() {
                        assets.retain(|asset| {
                            asset.extension.as_deref().unwrap_or("").eq_ignore_ascii_case(&extension)
                        });
                    }
                }

                if license_filter != "All" {
                    assets.retain(|asset| license_status_label(asset) == license_filter);
                }
                Ok((assets, search_note))
            })();
            let _ = tx.send(ClientEvent::Assets(result));
        });
    }

    fn refresh_projects(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        info!("project refresh started");
        thread::spawn(move || {
            let result = (|| -> Result<Vec<Project>, String> {
                let client = Self::api_client()?;
                client
                    .get(format!("{base}/api/projects"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::Projects(result));
        });
    }

    fn begin_upload(&mut self, path: PathBuf) {
        if !path.is_file() {
            warn!(path = %path.display(), "upload selection rejected because it is not a file");
            self.status = "Select a file to upload.".to_string();
            return;
        }

        let name = path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("New Asset")
            .to_string();

        info!(path = %path.display(), suggested_name = %name, "asset selected for upload");
        self.upload = UploadForm {
            file_path: Some(path),
            name,
            ..UploadForm::default()
        };
        self.show_upload = true;
    }

    fn submit_upload(&mut self) {
        let Some(file_path) = self.upload.file_path.clone() else {
            self.status = "No upload file selected.".to_string();
            return;
        };

        let tx = self.tx.clone();
        let base = self.base_url();
        let form_data = self.upload.clone();
        self.busy_count += 1;
        self.status = format!("Uploading {}...", file_path.display());
        info!(path = %file_path.display(), name = %form_data.name, "asset upload started");

        thread::spawn(move || {
            let result = upload_asset(&base, &form_data);
            let _ = tx.send(ClientEvent::Upload(result));
        });
    }

    fn open_versions_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        self.version_asset_id = Some(asset.id.clone());
        self.show_versions = true;
        self.refresh_versions(asset.id);
    }

    fn refresh_versions(&mut self, asset_id: String) {
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        info!(asset_id = %asset_id, "asset version history refresh started");
        thread::spawn(move || {
            let result = fetch_versions(&base, &asset_id);
            let _ = tx.send(ClientEvent::Versions { asset_id, result });
        });
    }

    fn upload_new_version(&mut self) {
        let Some(asset_id) = self.version_asset_id.clone() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Select replacement file for new version")
            .pick_file()
        else {
            return;
        };

        let tx = self.tx.clone();
        let base = self.base_url();
        let note = self.version_note.trim().to_string();
        self.busy_count += 1;
        self.status = format!("Uploading new version from {}...", path.display());
        info!(asset_id = %asset_id, path = %path.display(), "new asset version upload started");

        thread::spawn(move || {
            let result = upload_asset_version(&base, &asset_id, &path, &note);
            let _ = tx.send(ClientEvent::VersionChanged { asset_id, result });
        });
    }

    fn restore_version(&mut self, version_number: i64) {
        let Some(asset_id) = self.version_asset_id.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        self.status = format!("Restoring version {version_number}...");
        info!(asset_id = %asset_id, version_number, "asset version restore started");

        thread::spawn(move || {
            let result = restore_asset_version(&base, &asset_id, version_number);
            let _ = tx.send(ClientEvent::VersionChanged { asset_id, result });
        });
    }

    fn begin_package_import(&mut self, path: PathBuf) {
        if !path.is_file() {
            self.status = "Select a ZIP package.".to_string();
            return;
        }
        self.package_form = PackageForm {
            file_path: Some(path.clone()),
            name: path.file_stem().and_then(|v| v.to_str()).unwrap_or("Package").to_string(),
            ..PackageForm::default()
        };
        self.show_package_import = true;
        info!(path = %path.display(), "package selected for import");
    }

    fn submit_package_import(&mut self) {
        let Some(path) = self.package_form.file_path.clone() else {
            self.status = "No package ZIP selected.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let form = self.package_form.clone();
        self.busy_count += 1;
        self.status = format!("Importing package {}...", path.display());
        thread::spawn(move || {
            let result = import_package(&base, &form);
            let _ = tx.send(ClientEvent::PackageImported(result));
        });
    }

    fn refresh_package_manifest(&mut self, asset_id: String) {
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        info!(asset_id = %asset_id, "package manifest refresh started");
        thread::spawn(move || {
            let result = fetch_package_manifest(&base, &asset_id);
            let _ = tx.send(ClientEvent::PackageManifestLoaded { asset_id, result });
        });
    }

    fn open_package_contents(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let asset_id = asset.id.clone();
        self.show_package_contents = true;
        self.refresh_package_manifest(asset_id);
    }

    fn upload_package_version(&mut self) {
        let Some(asset_id) = self.version_asset_id.clone() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("Select package ZIP for new revision")
            .add_filter("ZIP package", &["zip"])
            .pick_file()
        else {
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let note = self.version_note.trim().to_string();
        self.busy_count += 1;
        self.status = format!("Uploading package revision {}...", path.display());
        thread::spawn(move || {
            let result = upload_package_version(&base, &asset_id, &path, &note);
            let _ = tx.send(ClientEvent::PackageVersionChanged { asset_id, result });
        });
    }

    fn refresh_project_license_files(&mut self) {
        let Some(project) = self.selected_project().cloned() else {
            self.status = "Select a project first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let project_id = project.id.clone();
        self.busy_count += 1;
        self.status = format!("Generating license files for {}...", project.name);
        thread::spawn(move || {
            let result = write_project_license_files(&base, &project);
            let _ = tx.send(ClientEvent::ProjectLicenseFiles { project_id, result });
        });
    }

    fn begin_edit_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        if asset.deleted_at.is_some() {
            self.status = "Restore this asset before editing it.".to_string();
            return;
        }

        self.edit = EditForm {
            asset_id: asset.id.clone(),
            name: asset.name,
            category: asset.category.unwrap_or_default(),
            tags: asset.tags.join(", "),
            description: asset.description.unwrap_or_default(),
            creator: asset.creator.unwrap_or_default(),
            source_url: asset.source_url.unwrap_or_default(),
            license: asset.license.unwrap_or_default(),
            attribution_required: asset.attribution_required,
        };
        info!(asset_id = %self.edit.asset_id, "asset edit dialog opened");
        self.show_edit = true;
    }

    fn submit_edit(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        let form = self.edit.clone();
        self.busy_count += 1;
        self.status = format!("Updating {}...", form.name);
        info!(asset_id = %form.asset_id, "asset metadata update started");

        thread::spawn(move || {
            let result = update_asset(&base, &form);
            let _ = tx.send(ClientEvent::Update(result));
        });
    }

    fn delete_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Moving {} to recycle bin...", asset.name);
        info!(asset_id = %id, "asset soft delete started");

        thread::spawn(move || {
            let result = delete_asset(&base, &id);
            let _ = tx.send(ClientEvent::Delete {
                asset_id: id,
                result,
            });
        });
    }

    fn restore_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select a deleted asset first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Restoring {}...", asset.name);
        info!(asset_id = %id, "asset restore started");

        thread::spawn(move || {
            let result = restore_asset(&base, &id);
            let _ = tx.send(ClientEvent::Restore {
                asset_id: id,
                result,
            });
        });
    }

    fn download_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };

        let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose download folder")
            .pick_folder()
        else {
            return;
        };

        let tx = self.tx.clone();
        let base = self.base_url();
        let asset_id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Downloading {}...", asset.name);

        thread::spawn(move || {
            let result = download_asset(&base, &asset, &folder);
            let _ = tx.send(ClientEvent::Download { asset_id, result });
        });
    }

    fn create_project(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        let form = self.project_form.clone();
        self.busy_count += 1;
        self.status = format!("Creating project {}...", form.name);
        info!(name = %form.name, local_path = %form.local_path, engine = %form.engine, "project create started");

        thread::spawn(move || {
            let result = create_project(&base, &form);
            let _ = tx.send(ClientEvent::ProjectCreated(result));
        });
    }

    fn add_selected_to_project(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let Some(project) = self.selected_project().cloned() else {
            self.status = "Select or create a project first.".to_string();
            return;
        };

        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        self.status = format!("Adding {} to {}...", asset.name, project.name);
        info!(asset_id = %asset.id, project_id = %project.id, "add to project started");

        thread::spawn(move || {
            let asset_id = asset.id.clone();
            let project_id = project.id.clone();
            let result = add_asset_to_project(&base, &project, &asset);
            let _ = tx.send(ClientEvent::ProjectAssetAdded {
                project_id,
                asset_id,
                result,
            });
        });
    }

    fn request_remove_selected_from_project(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let Some(project) = self.selected_project().cloned() else {
            self.status = "Select a project first.".to_string();
            return;
        };
        self.pending_remove_asset = Some(asset);
        self.pending_remove_project = Some(project);
        self.show_remove_project_confirm = true;
    }

    fn confirm_remove_selected_from_project(&mut self) {
        let Some(asset) = self.pending_remove_asset.take() else {
            self.show_remove_project_confirm = false;
            return;
        };
        let Some(project) = self.pending_remove_project.take() else {
            self.show_remove_project_confirm = false;
            return;
        };

        self.show_remove_project_confirm = false;
        let tx = self.tx.clone();
        let base = self.base_url();
        let asset_id = asset.id.clone();
        let project_id = project.id.clone();
        self.busy_count += 1;
        self.status = format!("Removing {} from {}...", asset.name, project.name);
        info!(asset_id = %asset_id, project_id = %project_id, "remove from project started");

        thread::spawn(move || {
            let result = remove_asset_from_project(&base, &project, &asset);
            let _ = tx.send(ClientEvent::ProjectAssetRemoved {
                project_id,
                asset_id,
                result,
            });
        });
    }

    fn selected_asset(&self) -> Option<&Asset> {
        let id = self.selected_id.as_deref()?;
        self.assets.iter().find(|a| a.id == id)
    }

    fn selected_project(&self) -> Option<&Project> {
        let id = self.selected_project_id.as_deref()?;
        self.projects.iter().find(|p| p.id == id)
    }

    fn request_missing_thumbnails(&mut self) {
        let candidates: Vec<String> = self
            .assets
            .iter()
            .filter(|asset| {
                asset.deleted_at.is_none() && is_previewable_extension(asset.extension.as_deref())
            })
            .map(|asset| asset.id.clone())
            .collect();

        for asset_id in candidates {
            if self.thumbnails.contains_key(&asset_id)
                || self.thumbnail_pending.contains(&asset_id)
            {
                continue;
            }

            self.thumbnail_pending.insert(asset_id.clone());
            let tx = self.tx.clone();
            let base = self.base_url();
            let request_id = asset_id.clone();

            thread::spawn(move || {
                let result = (|| -> Result<Vec<u8>, String> {
                    let client = Self::api_client()?;
                    let bytes = client
                        .get(format!("{base}/api/assets/{request_id}/preview"))
                        .send()
                        .map_err(|e| e.to_string())?
                        .error_for_status()
                        .map_err(|e| e.to_string())?
                        .bytes()
                        .map_err(|e| e.to_string())?;
                    Ok(bytes.to_vec())
                })();

                let _ = tx.send(ClientEvent::Thumbnail {
                    asset_id: request_id,
                    result,
                });
            });
        }
    }

    fn install_thumbnail(
        &mut self,
        ctx: &egui::Context,
        asset_id: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let decoded = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgba = decoded.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        let texture = ctx.load_texture(
            format!("thumbnail-{asset_id}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.thumbnails.insert(asset_id.to_string(), texture);
        Ok(())
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                ClientEvent::Health(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(health) => {
                            info!(phase = health.phase, version = %health.version, "server connection successful");
                            self.status = format!("Connected to {} v{}", health.service, health.version);
                            self.health = Some(health);
                        }
                        Err(err) => {
                            warn!(error = %err, "server connection failed");
                            self.health = None;
                            self.status = format!("Connection failed: {err}");
                        }
                    }
                }
                ClientEvent::SemanticStatus(result) => {
                    match result {
                        Ok(status) => {
                            info!(
                                enabled = status.enabled,
                                reachable = status.ollama_reachable,
                                indexed = status.indexed_assets,
                                total = status.total_active_assets,
                                stale = status.stale_assets,
                                model = %status.model,
                                "semantic status loaded"
                            );
                            self.semantic_status = Some(status);
                        }
                        Err(err) => {
                            warn!(error = %err, "semantic status unavailable");
                            self.semantic_status = None;
                        }
                    }
                }
                ClientEvent::SemanticReindex(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(response) => {
                            info!(
                                requested = response.requested,
                                indexed = response.indexed,
                                failed = response.failed,
                                model = %response.model,
                                "semantic reindex completed"
                            );
                            self.status = format!(
                                "Semantic index: {}/{} indexed, {} failed",
                                response.indexed, response.requested, response.failed
                            );
                            self.refresh_semantic_status();
                            self.refresh_assets();
                        }
                        Err(err) => {
                            warn!(error = %err, "semantic reindex failed");
                            self.status = format!("Semantic reindex failed: {err}");
                            self.refresh_semantic_status();
                        }
                    }
                }
                ClientEvent::BackupStatus(result) => {
                    match result {
                        Ok(status) => {
                            info!(
                                backup_directory = %status.backup_directory,
                                backups = status.backups.len(),
                                replicas = status.replication_targets.len(),
                                keep = status.keep,
                                "backup status loaded"
                            );
                            self.backup_status = Some(status);
                        }
                        Err(err) => {
                            warn!(error = %err, "backup status unavailable");
                            self.backup_status = None;
                        }
                    }
                }
                ClientEvent::BackupCreated(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(response) => {
                            info!(
                                backup_id = %response.backup.backup_id,
                                path = %response.backup.path,
                                total_bytes = response.backup.total_bytes,
                                files = response.backup.files,
                                replicas = response.replicated_to.len(),
                                replication_failures = response.replication_failures.len(),
                                "vault backup completed"
                            );
                            self.status = format!(
                                "Backup {} verified · {} · {} files · {} replicas{}",
                                response.backup.backup_id,
                                human_size(response.backup.total_bytes as i64),
                                response.backup.files,
                                response.replicated_to.len(),
                                if response.replication_failures.is_empty() {
                                    String::new()
                                } else {
                                    format!(" · {} replication failures", response.replication_failures.len())
                                }
                            );
                            self.refresh_backup_status();
                        }
                        Err(err) => {
                            warn!(error = %err, "vault backup failed");
                            self.status = format!("Backup failed: {err}");
                            self.refresh_backup_status();
                        }
                    }
                }
                ClientEvent::BackupVerified(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(response) => {
                            info!(
                                backup_id = %response.backup_id,
                                valid = response.valid,
                                checked_files = response.checked_files,
                                missing = response.missing_files.len(),
                                corrupt = response.corrupt_files.len(),
                                "backup verification completed"
                            );
                            self.status = if response.valid {
                                format!(
                                    "Backup {} verified successfully · {} files checked",
                                    response.backup_id, response.checked_files
                                )
                            } else {
                                format!(
                                    "Backup {} FAILED verification · {} missing · {} corrupt",
                                    response.backup_id,
                                    response.missing_files.len(),
                                    response.corrupt_files.len()
                                )
                            };
                            self.refresh_backup_status();
                        }
                        Err(err) => {
                            warn!(error = %err, "backup verification failed");
                            self.status = format!("Backup verification failed: {err}");
                        }
                    }
                }
                ClientEvent::Assets(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok((assets, search_note)) => {
                            info!(count = assets.len(), deleted_only = self.deleted_only, "asset refresh completed");
                            self.assets = assets;
                            if self
                                .selected_id
                                .as_ref()
                                .is_some_and(|id| !self.assets.iter().any(|a| &a.id == id))
                            {
                                self.selected_id = None;
                            }
                            self.status = match search_note {
                                Some(note) => format!("{} · {} results", note, self.assets.len()),
                                None => format!("{} assets loaded", self.assets.len()),
                            };
                            self.request_missing_thumbnails();
                        }
                        Err(err) => {
                            warn!(error = %err, "asset refresh failed");
                            self.status = format!("Could not load assets: {err}");
                        }
                    }
                }
                ClientEvent::Projects(result) => match result {
                    Ok(projects) => {
                        info!(count = projects.len(), "project refresh completed");
                        self.projects = projects;
                        if self
                            .selected_project_id
                            .as_ref()
                            .is_some_and(|id| !self.projects.iter().any(|p| &p.id == id))
                        {
                            self.selected_project_id = None;
                        }
                    }
                    Err(err) => {
                        warn!(error = %err, "project refresh failed");
                        self.status = format!("Could not load projects: {err}");
                    }
                },
                ClientEvent::Upload(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(response) => {
                            self.show_upload = false;
                            self.selected_id = Some(response.asset.id.clone());
                            if response.duplicate {
                                self.status = format!("Already in vault: {}", response.asset.name);
                                info!(asset_id = %response.asset.id, "duplicate upload blocked");
                            } else {
                                self.status = format!("Added to vault: {}", response.asset.name);
                                info!(asset_id = %response.asset.id, "asset upload completed");
                            }
                            self.refresh_assets();
                        }
                        Err(err) => {
                            warn!(error = %err, "asset upload failed");
                            self.status = format!("Upload failed: {err}");
                        }
                    }
                }
                ClientEvent::Update(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(asset) => {
                            info!(asset_id = %asset.id, "asset metadata update completed");
                            self.status = format!("Updated {}", asset.name);
                            self.selected_id = Some(asset.id.clone());
                            self.show_edit = false;
                            self.refresh_assets();
                        }
                        Err(err) => {
                            warn!(error = %err, "asset metadata update failed");
                            self.status = format!("Update failed: {err}");
                        }
                    }
                }
                ClientEvent::Delete { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(()) => {
                            info!(asset_id = %asset_id, "asset moved to recycle bin");
                            self.status = "Asset moved to recycle bin".to_string();
                            self.selected_id = None;
                            self.refresh_assets();
                        }
                        Err(err) => self.status = format!("Delete failed: {err}"),
                    }
                }
                ClientEvent::Restore { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(()) => {
                            info!(asset_id = %asset_id, "asset restored");
                            self.status = "Asset restored".to_string();
                            self.selected_id = None;
                            self.refresh_assets();
                        }
                        Err(err) => self.status = format!("Restore failed: {err}"),
                    }
                }
                ClientEvent::Download { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(path) => {
                            info!(asset_id = %asset_id, path = %path.display(), "asset download completed");
                            self.status = format!("Downloaded to {}", path.display());
                        }
                        Err(err) => self.status = format!("Download failed: {err}"),
                    }
                }
                ClientEvent::Thumbnail { asset_id, result } => {
                    self.thumbnail_pending.remove(&asset_id);
                    match result {
                        Ok(bytes) => {
                            if let Err(err) = self.install_thumbnail(ctx, &asset_id, &bytes) {
                                warn!(asset_id = %asset_id, error = %err, "preview decode failed");
                            } else {
                                info!(asset_id = %asset_id, byte_size = bytes.len(), "preview loaded");
                            }
                        }
                        Err(err) => {
                            warn!(asset_id = %asset_id, error = %err, "preview fetch failed");
                        }
                    }
                }
                ClientEvent::ProjectCreated(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(project) => {
                            info!(project_id = %project.id, "project created");
                            self.selected_project_id = Some(project.id.clone());
                            self.status = format!("Created project {}", project.name);
                            self.project_form = ProjectForm::default();
                            self.show_project_create = false;
                            self.refresh_projects();
                        }
                        Err(err) => self.status = format!("Project creation failed: {err}"),
                    }
                }
                ClientEvent::ProjectAssetAdded {
                    project_id,
                    asset_id,
                    result,
                } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(path) => {
                            info!(project_id = %project_id, asset_id = %asset_id, path = %path.display(), "asset added to project");
                            self.status = format!("Added asset to project: {}", path.display());
                        }
                        Err(err) => self.status = format!("Add to project failed: {err}"),
                    }
                }
                ClientEvent::ProjectAssetRemoved {
                    project_id,
                    asset_id,
                    result,
                } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(message) => {
                            info!(project_id = %project_id, asset_id = %asset_id, "asset removed from project");
                            self.status = message;
                        }
                        Err(err) => {
                            warn!(project_id = %project_id, asset_id = %asset_id, error = %err, "remove from project failed");
                            self.status = format!("Remove from project failed: {err}");
                        }
                    }
                }
                ClientEvent::Versions { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(versions) => {
                            info!(asset_id = %asset_id, count = versions.len(), "asset version history loaded");
                            self.versions = versions;
                        }
                        Err(err) => {
                            warn!(asset_id = %asset_id, error = %err, "asset version history failed");
                            self.status = format!("Version history failed: {err}");
                        }
                    }
                }
                ClientEvent::VersionChanged { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(asset) => {
                            info!(asset_id = %asset_id, version = asset.current_version, "asset version changed");
                            self.status = format!("{} is now version {}", asset.name, asset.current_version);
                            self.thumbnails.remove(&asset_id);
                            self.thumbnail_pending.remove(&asset_id);
                            self.version_note.clear();
                            self.refresh_versions(asset_id.clone());
                            self.refresh_package_manifest(asset_id.clone());
                            self.refresh_assets();
                        }
                        Err(err) => {
                            warn!(asset_id = %asset_id, error = %err, "asset version change failed");
                            self.status = format!("Version operation failed: {err}");
                        }
                    }
                }
                ClientEvent::ProjectLicenseFiles { project_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok((path, warnings, unknown)) => {
                            info!(project_id = %project_id, path = %path.display(), warnings, unknown, "project license files generated");
                            self.status = format!(
                                "License files updated: {} ({} warnings, {} unknown)",
                                path.display(), warnings, unknown
                            );
                        }
                        Err(err) => {
                            warn!(project_id = %project_id, error = %err, "project license files failed");
                            self.status = format!("License report failed: {err}");
                        }
                    }
                }
                ClientEvent::PackageImported(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(response) => {
                            info!(
                                asset_id = %response.asset.id,
                                files = response.manifest.files.len(),
                                missing = response.manifest.missing_dependencies.len(),
                                "package import completed"
                            );
                            self.selected_id = Some(response.asset.id.clone());
                            self.package_manifest = Some(response.manifest);
                            self.show_package_import = false;
                            self.package_form = PackageForm::default();
                            self.status = format!("Imported package {}", response.asset.name);
                            self.refresh_assets();
                        }
                        Err(err) => self.status = format!("Package import failed: {err}"),
                    }
                }
                ClientEvent::PackageManifestLoaded { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(manifest) => {
                            info!(asset_id = %asset_id, files = manifest.files.len(), missing = manifest.missing_dependencies.len(), "package manifest loaded");
                            self.package_manifest = Some(manifest);
                        }
                        Err(err) => {
                            self.status = format!("Package manifest failed: {err}");
                            self.package_manifest = None;
                        }
                    }
                }
                ClientEvent::PackageVersionChanged { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(response) => {
                            info!(asset_id = %asset_id, version = response.asset.current_version, files = response.manifest.files.len(), "package version uploaded");
                            self.package_manifest = Some(response.manifest);
                            self.thumbnails.remove(&asset_id);
                            self.thumbnail_pending.remove(&asset_id);
                            self.version_note.clear();
                            self.status = format!("{} is now package version {}", response.asset.name, response.asset.current_version);
                            self.refresh_versions(asset_id.clone());
                            self.refresh_assets();
                        }
                        Err(err) => self.status = format!("Package version failed: {err}"),
                    }
                }
            }
            ctx.request_repaint();
        }
    }

    fn process_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
            info!(path = %path.display(), "file dropped onto client");
            self.begin_upload(path);
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("DragonForge Asset Vault");
            ui.separator();
            ui.label("Server:");
            let response = ui.text_edit_singleline(&mut self.settings.server_url);
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let _ = save_settings(&self.settings);
                self.check_server();
                self.refresh_semantic_status();
                self.refresh_backup_status();
                self.refresh_assets();
                self.refresh_projects();
            }
            if ui.button("Connect").clicked() {
                let _ = save_settings(&self.settings);
                self.check_server();
                self.refresh_assets();
                self.refresh_projects();
            }
            if ui.button("Refresh").clicked() {
                self.refresh_backup_status();
                self.refresh_assets();
                self.refresh_projects();
            }
            ui.separator();
            let backup_label = match &self.backup_status {
                Some(status) => match status.backups.first() {
                    Some(latest) => format!(
                        "Backup: {} · {}",
                        latest.backup_id,
                        human_size(latest.total_bytes as i64)
                    ),
                    None => "Backup: none".to_string(),
                },
                None => "Backup: status unknown".to_string(),
            };
            ui.label(backup_label);
            if ui
                .add_enabled(self.busy_count == 0, egui::Button::new("Create Backup"))
                .clicked()
            {
                self.create_vault_backup();
            }
            if ui
                .add_enabled(
                    self.busy_count == 0
                        && self.backup_status.as_ref().is_some_and(|status| !status.backups.is_empty()),
                    egui::Button::new("Verify Latest"),
                )
                .clicked()
            {
                self.verify_latest_backup();
            }
            if ui.button("Backup Status").clicked() {
                self.refresh_backup_status();
            }
            if self.busy_count > 0 {
                ui.spinner();
            }
        });

        ui.horizontal(|ui| {
            let connection = match &self.health {
                Some(h) if h.ok => format!("Connected · server phase {} · v{}", h.phase, h.version),
                _ => "Not connected".to_string(),
            };
            ui.label(connection);
            ui.separator();
            ui.label(&self.status);
        });
    }

    fn filter_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Search");
            let search_response = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .desired_width(160.0)
                    .hint_text("name, creator, license..."),
            );
            egui::ComboBox::from_id_salt("search_mode")
                .selected_text(&self.search_mode)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.search_mode, "Smart".to_string(), "Smart");
                    ui.selectable_value(&mut self.search_mode, "Keyword".to_string(), "Keyword");
                });
            ui.label("Category");
            let category_response = ui.add(
                egui::TextEdit::singleline(&mut self.category_filter)
                    .desired_width(110.0)
                    .hint_text("texture"),
            );
            ui.label("Tag");
            let tag_response = ui.add(
                egui::TextEdit::singleline(&mut self.tag_filter)
                    .desired_width(110.0)
                    .hint_text("wood"),
            );
            ui.label("Type");
            let extension_response = ui.add(
                egui::TextEdit::singleline(&mut self.extension_filter)
                    .desired_width(75.0)
                    .hint_text("obj"),
            );
            ui.label("License");
            egui::ComboBox::from_id_salt("license_status_filter")
                .selected_text(&self.license_filter)
                .show_ui(ui, |ui| {
                    for value in ["All", "Complete", "Warning", "Unknown", "Custom"] {
                        ui.selectable_value(&mut self.license_filter, value.to_string(), value);
                    }
                });

            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Search").clicked()
                || (enter
                    && (search_response.has_focus()
                        || category_response.has_focus()
                        || tag_response.has_focus()
                        || extension_response.has_focus()))
            {
                self.refresh_assets();
            }
            if ui.button("Clear").clicked() {
                self.search.clear();
                self.category_filter.clear();
                self.tag_filter.clear();
                self.extension_filter.clear();
                self.license_filter = "All".to_string();
                self.refresh_assets();
            }

            ui.separator();
            let semantic_label = match &self.semantic_status {
                Some(status) if status.ollama_reachable => format!(
                    "AI: {}/{} indexed{}",
                    status.indexed_assets,
                    status.total_active_assets,
                    if status.stale_assets > 0 {
                        format!(" · {} stale", status.stale_assets)
                    } else {
                        String::new()
                    }
                ),
                Some(status) if !status.enabled => "AI: disabled".to_string(),
                Some(_) => "AI: Ollama offline".to_string(),
                None => "AI: status unknown".to_string(),
            };
            ui.label(semantic_label);
            if ui.button("Reindex AI Search").clicked() {
                self.reindex_semantic_search();
            }
            if ui.button("AI Status").clicked() {
                self.refresh_semantic_status();
            }

            let recycle_label = if self.deleted_only {
                "Back to Library"
            } else {
                "Recycle Bin"
            };
            if ui.button(recycle_label).clicked() {
                self.deleted_only = !self.deleted_only;
                self.selected_id = None;
                self.refresh_assets();
            }

            ui.separator();
            if ui.button("Add Asset").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select an asset to add")
                    .pick_file()
                {
                    self.begin_upload(path);
                }
            }
            if ui.button("Add Package ZIP").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select a multi-file asset ZIP")
                    .add_filter("ZIP package", &["zip"])
                    .pick_file()
                {
                    self.begin_package_import(path);
                }
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Project:");
            let selected_name = self
                .selected_project()
                .map(|p| format!("{} ({})", p.name, p.engine))
                .unwrap_or_else(|| "None selected".to_string());

            egui::ComboBox::from_id_salt("project_selector")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for project in &self.projects {
                        ui.selectable_value(
                            &mut self.selected_project_id,
                            Some(project.id.clone()),
                            format!("{} ({})", project.name, project.engine),
                        );
                    }
                });

            if ui.button("New Project").clicked() {
                self.show_project_create = true;
            }
            if ui
                .add_enabled(
                    self.selected_project_id.is_some(),
                    egui::Button::new("Refresh Credits / License Manifest"),
                )
                .clicked()
            {
                self.refresh_project_license_files();
            }

            if ui
                .add_enabled(
                    self.selected_id.is_some()
                        && self.selected_project_id.is_some()
                        && !self.deleted_only,
                    egui::Button::new("Add Selected to Project"),
                )
                .clicked()
            {
                self.add_selected_to_project();
            }
            if ui
                .add_enabled(
                    self.selected_id.is_some()
                        && self.selected_project_id.is_some()
                        && !self.deleted_only,
                    egui::Button::new("Remove Selected from Project"),
                )
                .clicked()
            {
                self.request_remove_selected_from_project();
            }
        });
    }

    fn asset_grid(&mut self, ui: &mut egui::Ui) {
        let mut chosen: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("dragonforge_asset_grid")
                .num_columns(4)
                .spacing([12.0, 12.0])
                .show(ui, |ui| {
                    for (index, asset) in self.assets.iter().enumerate() {
                        ui.vertical(|ui| {
                            if let Some(texture) = self.thumbnails.get(&asset.id) {
                                ui.image((texture.id(), egui::vec2(210.0, 120.0)));
                            } else {
                                let placeholder = match asset.extension.as_deref() {
                                    Some("fbx") | Some("obj") | Some("glb") | Some("gltf")
                                    | Some("blend") => "3D MODEL",
                                    Some("wav") | Some("mp3") | Some("ogg") => "AUDIO",
                                    Some("zip") | Some("7z") => "ARCHIVE",
                                    _ => "ASSET",
                                };
                                ui.add_sized(
                                    [210.0, 120.0],
                                    egui::Label::new(placeholder).selectable(false),
                                );
                            }

                            let ext = asset
                                .extension
                                .as_deref()
                                .unwrap_or("file")
                                .to_ascii_uppercase();
                            let category = asset.category.as_deref().unwrap_or("Uncategorized");
                            let marker = if self.selected_id.as_deref() == Some(asset.id.as_str()) {
                                "● "
                            } else {
                                ""
                            };
                            let deleted = if asset.deleted_at.is_some() {
                                "\n[RECYCLE BIN]"
                            } else {
                                ""
                            };
                            let license_status = license_status_label(asset);
                            let label = format!(
                                "{marker}{}\n{} · {}\n{}\nLicense: {}\n{}{}",
                                asset.name,
                                ext,
                                human_size(asset.byte_size),
                                category,
                                license_status,
                                if asset.tags.is_empty() {
                                    "No tags".to_string()
                                } else {
                                    asset.tags.join(", ")
                                },
                                deleted
                            );

                            if ui
                                .add_sized([210.0, 92.0], egui::Button::new(label).wrap())
                                .clicked()
                            {
                                chosen = Some(asset.id.clone());
                            }
                        });

                        if (index + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
        });

        if let Some(id) = chosen {
            info!(asset_id = %id, "asset selected");
            self.selected_id = Some(id);
        }
    }

    fn details_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(if self.deleted_only {
            "Recycle Bin Details"
        } else {
            "Asset Details"
        });
        ui.separator();

        let Some(asset) = self.selected_asset().cloned() else {
            ui.label("Select an asset from the library.");
            return;
        };

        if let Some(texture) = self.thumbnails.get(&asset.id) {
            let available = ui.available_width().min(300.0);
            ui.image((texture.id(), egui::vec2(available, available * 0.7)));
            ui.separator();
        }

        ui.heading(&asset.name);
        ui.label(format!("File: {}", asset.original_filename));
        ui.label(format!("Size: {}", human_size(asset.byte_size)));
        ui.label(format!("Type: {}", asset.extension.as_deref().unwrap_or("unknown")));
        ui.label(format!(
            "Category: {}",
            asset.category.as_deref().unwrap_or("Uncategorized")
        ));
        ui.label(format!("Added: {}", asset.created_at));
        ui.label(format!("Updated: {}", asset.updated_at));
        ui.label(format!("Current Version: v{}", asset.current_version));
        if let Some(deleted_at) = &asset.deleted_at {
            ui.label(format!("Deleted: {deleted_at}"));
        }
        ui.separator();

        if let Some(description) = &asset.description {
            ui.label(description);
            ui.separator();
        }

        ui.label(format!(
            "Tags: {}",
            if asset.tags.is_empty() {
                "None".to_string()
            } else {
                asset.tags.join(", ")
            }
        ));
        ui.label(format!("Creator: {}", asset.creator.as_deref().unwrap_or("Unknown")));
        ui.label(format!(
            "License: {}",
            asset.license.as_deref().unwrap_or("Not recorded")
        ));
        ui.label(format!(
            "Attribution: {}",
            if asset.attribution_required {
                "Required"
            } else {
                "Not marked as required"
            }
        ));
        let license_status = license_status_label(&asset);
        ui.label(format!("License Status: {license_status}"));
        for warning in license_warnings(&asset) {
            ui.label(format!("⚠ {warning}"));
        }
        if let Some(source) = &asset.source_url {
            ui.label(format!("Source: {source}"));
        }

        ui.separator();
        ui.label("SHA-256");
        ui.small(&asset.sha256);
        ui.add_space(8.0);

        if asset.deleted_at.is_some() {
            if ui.button("Restore Asset").clicked() {
                self.restore_selected();
            }
        } else {
            if ui.button("Download Asset").clicked() {
                self.download_selected();
            }
            if ui.button("Edit Metadata").clicked() {
                self.begin_edit_selected();
            }
            if ui.button("Version History").clicked() {
                self.open_versions_selected();
            }
            if ui.button("Package / Dependencies").clicked() {
                self.open_package_contents();
            }
            if ui.button("Move to Recycle Bin").clicked() {
                self.delete_selected();
            }
            if ui
                .add_enabled(
                    self.selected_project_id.is_some(),
                    egui::Button::new("Add to Selected Project"),
                )
                .clicked()
            {
                self.add_selected_to_project();
            }
            if ui
                .add_enabled(
                    self.selected_project_id.is_some(),
                    egui::Button::new("Remove from Selected Project"),
                )
                .clicked()
            {
                self.request_remove_selected_from_project();
            }
        }
    }

    fn upload_window(&mut self, ctx: &egui::Context) {
        if !self.show_upload {
            return;
        }

        let mut open = self.show_upload;
        egui::Window::new("Add Asset to DragonForge")
            .open(&mut open)
            .resizable(true)
            .default_width(540.0)
            .show(ctx, |ui| {
                if let Some(path) = &self.upload.file_path {
                    ui.label(format!("File: {}", path.display()));
                }
                ui.separator();
                form_row(ui, "Name", &mut self.upload.name);
                form_row(ui, "Category", &mut self.upload.category);
                form_row(ui, "Tags", &mut self.upload.tags);
                form_row(ui, "Creator", &mut self.upload.creator);
                form_row(ui, "Source URL", &mut self.upload.source_url);
                license_combo(ui, &mut self.upload.license, &mut self.upload.attribution_required);
                ui.checkbox(&mut self.upload.attribution_required, "Attribution required");
                ui.label("Description");
                ui.add(
                    egui::TextEdit::multiline(&mut self.upload.description)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                ui.separator();
                if ui
                    .add_enabled(self.busy_count == 0, egui::Button::new("Upload to Vault"))
                    .clicked()
                {
                    self.submit_upload();
                }
            });
        self.show_upload = open && self.show_upload;
    }

    fn edit_window(&mut self, ctx: &egui::Context) {
        if !self.show_edit {
            return;
        }

        let mut open = self.show_edit;
        egui::Window::new("Edit Asset Metadata")
            .open(&mut open)
            .resizable(true)
            .default_width(540.0)
            .show(ctx, |ui| {
                form_row(ui, "Name", &mut self.edit.name);
                form_row(ui, "Category", &mut self.edit.category);
                form_row(ui, "Tags", &mut self.edit.tags);
                form_row(ui, "Creator", &mut self.edit.creator);
                form_row(ui, "Source URL", &mut self.edit.source_url);
                license_combo(ui, &mut self.edit.license, &mut self.edit.attribution_required);
                ui.checkbox(&mut self.edit.attribution_required, "Attribution required");
                ui.label("Description");
                ui.add(
                    egui::TextEdit::multiline(&mut self.edit.description)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                ui.separator();
                if ui
                    .add_enabled(self.busy_count == 0, egui::Button::new("Save Changes"))
                    .clicked()
                {
                    self.submit_edit();
                }
            });
        self.show_edit = open && self.show_edit;
    }

    fn versions_window(&mut self, ctx: &egui::Context) {
        if !self.show_versions {
            return;
        }

        let mut open = self.show_versions;
        let current_version = self
            .selected_asset()
            .map(|asset| asset.current_version)
            .unwrap_or_default();
        let versions = self.versions.clone();

        egui::Window::new("Asset Version History")
            .open(&mut open)
            .resizable(true)
            .default_width(620.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Current: v{current_version}"));
                    if ui.button("Refresh History").clicked() {
                        if let Some(id) = self.version_asset_id.clone() {
                            self.refresh_versions(id);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("New version note:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.version_note)
                            .desired_width(320.0)
                            .hint_text("What changed?"),
                    );
                    if ui.button("Upload New Version").clicked() {
                        self.upload_new_version();
                    }
                    if ui.button("Upload Package Version ZIP").clicked() {
                        self.upload_package_version();
                    }
                });

                ui.separator();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for version in &versions {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.heading(format!("v{}", version.version_number));
                                if version.version_number == current_version {
                                    ui.label("CURRENT");
                                }
                                ui.label(format!(
                                    "{} · {}",
                                    version.original_filename,
                                    human_size(version.byte_size)
                                ));
                            });
                            ui.small(format!("SHA-256: {}", version.sha256));
                            ui.small(format!("Created: {}", version.created_at));
                            if let Some(note) = &version.note {
                                ui.label(format!("Note: {note}"));
                            }
                            if version.version_number != current_version
                                && ui.button(format!("Restore v{} as New Current Version", version.version_number)).clicked()
                            {
                                self.restore_version(version.version_number);
                            }
                        });
                        ui.add_space(6.0);
                    }
                });
            });

        self.show_versions = open;
    }

    fn package_import_window(&mut self, ctx: &egui::Context) {
        if !self.show_package_import {
            return;
        }
        let mut open = self.show_package_import;
        egui::Window::new("Import Multi-File Asset Package")
            .open(&mut open)
            .resizable(true)
            .default_width(600.0)
            .show(ctx, |ui| {
                if let Some(path) = &self.package_form.file_path {
                    ui.label(format!("ZIP: {}", path.display()));
                }
                ui.label("Leave Primary Path blank to auto-detect GLB/GLTF/OBJ.");
                form_row(ui, "Primary Path", &mut self.package_form.primary_path);
                form_row(ui, "Name", &mut self.package_form.name);
                form_row(ui, "Category", &mut self.package_form.category);
                form_row(ui, "Tags", &mut self.package_form.tags);
                form_row(ui, "Creator", &mut self.package_form.creator);
                form_row(ui, "Source URL", &mut self.package_form.source_url);
                license_combo(ui, &mut self.package_form.license, &mut self.package_form.attribution_required);
                ui.checkbox(&mut self.package_form.attribution_required, "Attribution required");
                ui.label("Description");
                ui.add(egui::TextEdit::multiline(&mut self.package_form.description)
                    .desired_rows(3).desired_width(f32::INFINITY));
                ui.separator();
                if ui.add_enabled(self.busy_count == 0, egui::Button::new("Import Package")).clicked() {
                    self.submit_package_import();
                }
            });
        self.show_package_import = open && self.show_package_import;
    }

    fn package_contents_window(&mut self, ctx: &egui::Context) {
        if !self.show_package_contents {
            return;
        }
        let mut open = self.show_package_contents;
        let manifest = self.package_manifest.clone();
        egui::Window::new("Package Contents & Dependencies")
            .open(&mut open)
            .resizable(true)
            .default_width(680.0)
            .show(ctx, |ui| {
                let Some(manifest) = manifest else {
                    ui.label("Loading package manifest...");
                    return;
                };
                ui.heading(format!("Version v{}", manifest.version_number));
                ui.label(format!("Primary: {}", manifest.primary_path));
                ui.label(format!("Files: {}", manifest.files.len()));
                if manifest.missing_dependencies.is_empty() {
                    ui.label("Dependency status: complete");
                } else {
                    ui.label(format!("Missing dependencies: {}", manifest.missing_dependencies.len()));
                    for missing in &manifest.missing_dependencies {
                        ui.label(format!("⚠ {missing}"));
                    }
                }
                ui.separator();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for file in &manifest.files {
                        ui.horizontal_wrapped(|ui| {
                            if file.is_primary { ui.label("PRIMARY"); }
                            ui.label(&file.relative_path);
                            ui.small(human_size(file.byte_size));
                        });
                    }
                });
            });
        self.show_package_contents = open;
    }

    fn remove_project_confirm_window(&mut self, ctx: &egui::Context) {
        if !self.show_remove_project_confirm {
            return;
        }

        let asset_name = self.pending_remove_asset.as_ref()
            .map(|asset| asset.name.clone())
            .unwrap_or_else(|| "this asset".to_string());
        let project_name = self.pending_remove_project.as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "the selected project".to_string());

        egui::Window::new("Remove Asset from Project")
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Remove '{}' from '{}'?",
                    asset_name, project_name
                ));
                ui.add_space(6.0);
                ui.label(
                    "DragonForge will remove the exported project copy. For a package, the entire exported package folder will be removed. The vault asset and its revision history will not be deleted."
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.busy_count == 0, egui::Button::new("Remove from Project"))
                        .clicked()
                    {
                        self.confirm_remove_selected_from_project();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_remove_project_confirm = false;
                        self.pending_remove_asset = None;
                        self.pending_remove_project = None;
                    }
                });
            });
    }

    fn project_window(&mut self, ctx: &egui::Context) {
        if !self.show_project_create {
            return;
        }

        let mut open = self.show_project_create;
        egui::Window::new("Create DragonForge Project")
            .open(&mut open)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                form_row(ui, "Project Name", &mut self.project_form.name);
                form_row(ui, "Engine", &mut self.project_form.engine);

                ui.horizontal(|ui| {
                    ui.label("Local Path:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_form.local_path)
                            .desired_width(330.0),
                    );
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("Choose project root folder")
                            .pick_folder()
                        {
                            self.project_form.local_path = path.display().to_string();
                        }
                    }
                });

                ui.label("Description");
                ui.add(
                    egui::TextEdit::multiline(&mut self.project_form.description)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );

                ui.label("Assets added to this project will be copied into a DragonForgeAssets folder under the selected project root.");

                ui.separator();
                if ui
                    .add_enabled(
                        self.busy_count == 0
                            && !self.project_form.name.trim().is_empty()
                            && !self.project_form.local_path.trim().is_empty(),
                        egui::Button::new("Create Project"),
                    )
                    .clicked()
                {
                    self.create_project();
                }
            });
        self.show_project_create = open && self.show_project_create;
    }
}

impl eframe::App for DragonForgeClient {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events(ctx);
        self.process_dropped_files(ctx);

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            self.top_bar(ui);
            ui.separator();
            self.filter_bar(ui);
        });

        egui::SidePanel::right("details")
            .default_width(350.0)
            .resizable(true)
            .show(ctx, |ui| self.details_panel(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.assets.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(if self.deleted_only {
                        "Recycle bin is empty."
                    } else {
                        "No matching assets. Add an asset or change the filters."
                    });
                });
            } else {
                self.asset_grid(ui);
            }
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} assets shown", self.assets.len()));
                ui.separator();
                ui.label(format!("{} projects", self.projects.len()));
                ui.separator();
                ui.label("DragonForge Client Phase 10");
                ui.separator();
                ui.label(format!("Logs: {}", client_log_dir().display()));
            });
        });

        self.upload_window(ctx);
        self.edit_window(ctx);
        self.project_window(ctx);
        self.versions_window(ctx);
        self.package_import_window(ctx);
        self.package_contents_window(ctx);
        self.remove_project_confirm_window(ctx);
    }
}

fn upload_asset(base: &str, data: &UploadForm) -> Result<UploadResponse, String> {
    let path = data
        .file_path
        .as_ref()
        .ok_or_else(|| "missing file path".to_string())?;

    let client = DragonForgeClient::api_client()?;
    let file_part = multipart::Part::file(path).map_err(|e| e.to_string())?;
    let mut form = multipart::Form::new().part("file", file_part);

    for (name, value) in [
        ("name", data.name.trim()),
        ("category", data.category.trim()),
        ("tags", data.tags.trim()),
        ("description", data.description.trim()),
        ("creator", data.creator.trim()),
        ("source_url", data.source_url.trim()),
        ("license", data.license.trim()),
    ] {
        if !value.is_empty() {
            form = form.text(name.to_string(), value.to_string());
        }
    }

    form = form.text(
        "attribution_required",
        data.attribution_required.to_string(),
    );

    client
        .post(format!("{base}/api/assets"))
        .multipart(form)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

fn update_asset(base: &str, data: &EditForm) -> Result<Asset, String> {
    let tags = data
        .tags
        .split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();

    let body = serde_json::json!({
        "name": data.name,
        "category": data.category,
        "description": data.description,
        "source_url": data.source_url,
        "creator": data.creator,
        "license": data.license,
        "attribution_required": data.attribution_required,
        "tags": tags
    });

    DragonForgeClient::api_client()?
        .patch(format!("{base}/api/assets/{}", data.asset_id))
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

fn delete_asset(base: &str, asset_id: &str) -> Result<(), String> {
    DragonForgeClient::api_client()?
        .delete(format!("{base}/api/assets/{asset_id}"))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn restore_asset(base: &str, asset_id: &str) -> Result<(), String> {
    DragonForgeClient::api_client()?
        .post(format!("{base}/api/assets/{asset_id}/restore"))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn download_asset(base: &str, asset: &Asset, folder: &Path) -> Result<PathBuf, String> {
    let client = DragonForgeClient::api_client()?;
    let mut response = client
        .get(format!("{base}/api/assets/{}/download", asset.id))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    let target = unique_download_path(folder, &asset.original_filename);
    let mut file = fs::File::create(&target).map_err(|e| e.to_string())?;
    io::copy(&mut response, &mut file).map_err(|e| e.to_string())?;
    Ok(target)
}

fn create_project(base: &str, data: &ProjectForm) -> Result<Project, String> {
    let body = serde_json::json!({
        "name": data.name,
        "engine": data.engine,
        "local_path": data.local_path,
        "description": data.description
    });

    DragonForgeClient::api_client()?
        .post(format!("{base}/api/projects"))
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

fn add_asset_to_project(base: &str, project: &Project, asset: &Asset) -> Result<PathBuf, String> {
    let client = DragonForgeClient::api_client()?;
    let manifest: PackageManifest = client
        .get(format!("{base}/api/assets/{}/package", asset.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let root = PathBuf::from(&project.local_path).join("DragonForgeAssets");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;

    let package_mode = manifest.files.len() > 1 || !manifest.referenced_dependencies.is_empty();
    let export_root = if package_mode {
        root.join(sanitize_component(&asset.name))
    } else {
        root.clone()
    };
    fs::create_dir_all(&export_root).map_err(|e| e.to_string())?;

    let mut primary_target = None;
    for file in &manifest.files {
        let target = if package_mode {
            export_root.join(Path::new(&file.relative_path))
        } else {
            export_root.join(&file.original_filename)
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut response = if file.id.starts_with("single-") {
            client.get(format!("{base}/api/assets/{}/download", asset.id))
        } else {
            client.get(format!(
                "{base}/api/assets/{}/versions/{}/package/files/{}/download",
                asset.id, manifest.version_number, file.id
            ))
        }
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?;

        let mut output = fs::File::create(&target).map_err(|e| e.to_string())?;
        io::copy(&mut response, &mut output).map_err(|e| e.to_string())?;
        if file.is_primary {
            primary_target = Some(target);
        }
    }

    let relative_path = primary_target
        .as_ref()
        .and_then(|path| path.strip_prefix(&project.local_path).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| format!("DragonForgeAssets/{}", asset.original_filename));

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

    let _ = write_project_license_files(base, project)?;

    Ok(primary_target.unwrap_or(export_root))
}

fn remove_asset_from_project(
    base: &str,
    project: &Project,
    asset: &Asset,
) -> Result<String, String> {
    let client = DragonForgeClient::api_client()?;
    let link: ProjectAsset = client
        .get(format!(
            "{base}/api/projects/{}/assets/{}",
            project.id, asset.id
        ))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let manifest: PackageManifest = client
        .get(format!(
            "{base}/api/assets/{}/versions/{}/package",
            asset.id, link.version_number
        ))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let relative = link.relative_path.as_deref()
        .ok_or_else(|| "project asset link does not contain an exported relative path".to_string())?;
    let primary_target = safe_project_export_path(project, relative)?;
    let package_mode = manifest.files.len() > 1 || !manifest.referenced_dependencies.is_empty();

    let removal_target = if package_mode {
        let primary_component_count = Path::new(&manifest.primary_path)
            .components()
            .filter(|component| matches!(component, Component::Normal(_)))
            .count();
        if primary_component_count == 0 {
            return Err("package primary path is invalid".to_string());
        }
        let mut root = primary_target.clone();
        for _ in 0..primary_component_count {
            if !root.pop() {
                return Err("could not determine exported package root".to_string());
            }
        }
        root
    } else {
        primary_target
    };

    let export_root = PathBuf::from(&project.local_path).join("DragonForgeAssets");
    if !removal_target.starts_with(&export_root) || removal_target == export_root {
        return Err("refusing to remove a path outside the project's DragonForgeAssets folder".to_string());
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
        fs::rename(&removal_target, &staged).map_err(|e| {
            format!("could not stage exported project copy for removal: {e}")
        })?;
        Some(staged)
    } else {
        None
    };

    let unlink_result = client
        .delete(format!(
            "{base}/api/projects/{}/assets/{}",
            project.id, asset.id
        ))
        .send()
        .map_err(|e| e.to_string())
        .and_then(|response| response.error_for_status().map_err(|e| e.to_string()));

    if let Err(err) = unlink_result {
        if let Some(staged) = &staging {
            let _ = fs::rename(staged, &removal_target);
        }
        return Err(format!("server unlink failed; project files were restored: {err}"));
    }

    if let Some(staged) = staging {
        let cleanup = if staged.is_dir() {
            fs::remove_dir_all(&staged)
        } else {
            fs::remove_file(&staged)
        };
        if let Err(err) = cleanup {
            return Err(format!(
                "project link was removed, but staged files could not be deleted at {}: {err}",
                staged.display()
            ));
        }
    }

    let credits_note = match write_project_license_files(base, project) {
        Ok(_) => String::new(),
        Err(err) => format!(" License files could not be refreshed automatically: {err}"),
    };

    Ok(if existed {
        format!(
            "Removed {} from {} and deleted {}.{}",
            asset.name,
            project.name,
            removal_target.display(),
            credits_note
        )
    } else {
        format!(
            "Removed {} from {}. The exported project copy was already missing.{}",
            asset.name,
            project.name,
            credits_note
        )
    })
}

fn safe_project_export_path(project: &Project, relative: &str) -> Result<PathBuf, String> {
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

    let target = PathBuf::from(&project.local_path).join(clean);
    let export_root = PathBuf::from(&project.local_path).join("DragonForgeAssets");
    if !target.starts_with(&export_root) {
        return Err("project asset path is outside DragonForgeAssets".to_string());
    }
    Ok(target)
}

fn write_project_license_files(
    base: &str,
    project: &Project,
) -> Result<(PathBuf, usize, usize), String> {
    let report: ProjectLicenseReport = DragonForgeClient::api_client()?
        .get(format!("{base}/api/projects/{}/license-report", project.id))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let root = PathBuf::from(&project.local_path);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let credits_path = root.join("CREDITS.txt");
    let json_path = root.join("DragonForge-License-Manifest.json");
    let csv_path = root.join("DragonForge-License-Manifest.csv");

    fs::write(&credits_path, &report.credits_text).map_err(|e| e.to_string())?;
    fs::write(
        &json_path,
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
    ).map_err(|e| e.to_string())?;
    fs::write(&csv_path, &report.csv_manifest).map_err(|e| e.to_string())?;

    Ok((credits_path, report.warning_count, report.unknown_count))
}

fn license_status_label(asset: &Asset) -> String {
    let license = asset.license.as_deref().unwrap_or("").trim();
    if license.is_empty() || license.eq_ignore_ascii_case("unknown") {
        return "Unknown".to_string();
    }
    if license.eq_ignore_ascii_case("custom") {
        return "Custom".to_string();
    }
    if license.eq_ignore_ascii_case("CC-BY-4.0")
        || license.eq_ignore_ascii_case("CC BY 4.0")
    {
        if asset.creator.as_deref().is_none_or(str::is_empty)
            || asset.source_url.as_deref().is_none_or(str::is_empty)
            || !asset.attribution_required
        {
            return "Warning".to_string();
        }
        return "Complete".to_string();
    }
    if license.eq_ignore_ascii_case("CC0-1.0")
        || license.eq_ignore_ascii_case("CC0")
    {
        if asset.source_url.as_deref().is_none_or(str::is_empty) {
            return "Warning".to_string();
        }
        return "Complete".to_string();
    }
    "Custom".to_string()
}

fn license_warnings(asset: &Asset) -> Vec<String> {
    let mut warnings = Vec::new();
    let license = asset.license.as_deref().unwrap_or("").trim();
    if license.is_empty() || license.eq_ignore_ascii_case("unknown") {
        warnings.push("License has not been verified.".to_string());
    } else if license.eq_ignore_ascii_case("CC-BY-4.0") || license.eq_ignore_ascii_case("CC BY 4.0") {
        if asset.creator.as_deref().is_none_or(str::is_empty) {
            warnings.push("Missing creator/credit name.".to_string());
        }
        if asset.source_url.as_deref().is_none_or(str::is_empty) {
            warnings.push("Missing source URL.".to_string());
        }
        if !asset.attribution_required {
            warnings.push("CC-BY-4.0 should be marked as requiring attribution.".to_string());
        }
    } else if (license.eq_ignore_ascii_case("CC0-1.0") || license.eq_ignore_ascii_case("CC0"))
        && asset.source_url.as_deref().is_none_or(str::is_empty)
    {
        warnings.push("Source URL is missing; provenance cannot be verified later.".to_string());
    }
    warnings
}

fn license_combo(ui: &mut egui::Ui, value: &mut String, attribution_required: &mut bool) {
    let selected = if value.trim().is_empty() {
        "Unknown".to_string()
    } else {
        value.clone()
    };
    ui.horizontal(|ui| {
        ui.label("License:");
        egui::ComboBox::from_id_salt(ui.next_auto_id())
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for (id, label, requires_attribution) in [
                    ("CC0-1.0", "CC0-1.0 — Creative Commons Zero", false),
                    ("CC-BY-4.0", "CC-BY-4.0 — Creative Commons Attribution 4.0", true),
                    ("Custom", "Custom / site-specific", false),
                    ("Unknown", "Unknown / not verified", false),
                ] {
                    if ui.selectable_label(value == id, label).clicked() {
                        *value = id.to_string();
                        *attribution_required = requires_attribution;
                    }
                }
            });
    });
}

fn sanitize_component(value: &str) -> String {
    value.chars()
        .map(|ch| if ['<','>',':','"','/','\\','|','?','*'].contains(&ch) { '_' } else { ch })
        .collect()
}

fn fetch_versions(base: &str, asset_id: &str) -> Result<Vec<AssetVersion>, String> {
    DragonForgeClient::api_client()?
        .get(format!("{base}/api/assets/{asset_id}/versions"))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

fn upload_asset_version(
    base: &str,
    asset_id: &str,
    path: &Path,
    note: &str,
) -> Result<Asset, String> {
    let client = DragonForgeClient::api_client()?;
    let file_part = multipart::Part::file(path).map_err(|e| e.to_string())?;
    let mut form = multipart::Form::new().part("file", file_part);
    if !note.is_empty() {
        form = form.text("note", note.to_string());
    }

    client
        .post(format!("{base}/api/assets/{asset_id}/versions"))
        .multipart(form)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

fn restore_asset_version(
    base: &str,
    asset_id: &str,
    version_number: i64,
) -> Result<Asset, String> {
    DragonForgeClient::api_client()?
        .post(format!(
            "{base}/api/assets/{asset_id}/versions/{version_number}/restore"
        ))
        .json(&serde_json::json!({
            "note": format!("Restored from version {version_number} via desktop client")
        }))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())
}

fn import_package(base: &str, data: &PackageForm) -> Result<PackageImportResponse, String> {
    let path = data.file_path.as_ref().ok_or_else(|| "missing ZIP path".to_string())?;
    let client = DragonForgeClient::api_client()?;
    let file_part = multipart::Part::file(path).map_err(|e| e.to_string())?;
    let mut form = multipart::Form::new().part("file", file_part);
    for (name, value) in [
        ("primary_path", data.primary_path.trim()),
        ("name", data.name.trim()),
        ("category", data.category.trim()),
        ("tags", data.tags.trim()),
        ("description", data.description.trim()),
        ("creator", data.creator.trim()),
        ("source_url", data.source_url.trim()),
        ("license", data.license.trim()),
    ] {
        if !value.is_empty() {
            form = form.text(name.to_string(), value.to_string());
        }
    }
    form = form.text("attribution_required", data.attribution_required.to_string());

    client.post(format!("{base}/api/packages"))
        .multipart(form).send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())
}

fn fetch_package_manifest(base: &str, asset_id: &str) -> Result<PackageManifest, String> {
    DragonForgeClient::api_client()?
        .get(format!("{base}/api/assets/{asset_id}/package"))
        .send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())
}

fn upload_package_version(
    base: &str,
    asset_id: &str,
    path: &Path,
    note: &str,
) -> Result<PackageVersionResponse, String> {
    let client = DragonForgeClient::api_client()?;
    let file_part = multipart::Part::file(path).map_err(|e| e.to_string())?;
    let mut form = multipart::Form::new().part("file", file_part);
    if !note.is_empty() {
        form = form.text("note", note.to_string());
    }
    client.post(format!("{base}/api/assets/{asset_id}/package-versions"))
        .multipart(form).send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())
}

fn unique_download_path(folder: &Path, filename: &str) -> PathBuf {
    let safe = filename.replace(['/', '\\'], "_");
    let candidate = folder.join(&safe);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(&safe);
    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("asset");
    let ext = path.extension().and_then(|v| v.to_str());

    for index in 1..10_000 {
        let name = match ext {
            Some(ext) => format!("{stem}_{index}.{ext}"),
            None => format!("{stem}_{index}"),
        };
        let candidate = folder.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }

    folder.join(format!("{stem}_download"))
}

fn form_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.set_min_width(500.0);
        ui.label(format!("{label}:"));
        ui.add(egui::TextEdit::singleline(value).desired_width(380.0));
    });
}

fn human_size(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", value as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn is_previewable_extension(extension: Option<&str>) -> bool {
    matches!(
        extension.map(|v| v.to_ascii_lowercase()).as_deref(),
        Some("jpg") | Some("jpeg") | Some("png") | Some("webp") | Some("obj") | Some("glb") | Some("gltf")
    )
}

fn settings_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata)
            .join("DragonForge")
            .join("AssetVault")
            .join("client.json");
    }
    PathBuf::from("DragonForgeClient.json")
}

fn client_log_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata)
            .join("DragonForge")
            .join("AssetVault")
            .join("logs");
    }
    PathBuf::from("logs")
}

fn load_settings() -> Result<ClientSettings, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(ClientSettings::default());
    }
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn save_settings(settings: &ClientSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn main() -> eframe::Result<()> {
    let log_dir = client_log_dir();
    let _log_guard = match logging::init_file_logging(
        &log_dir,
        "client.log",
        "dragonforge_client=info,info",
    ) {
        Ok(guard) => Some(guard),
        Err(err) => {
            eprintln!("DragonForge client logger failed to initialize: {err}");
            None
        }
    };

    info!(
        version = env!("CARGO_PKG_VERSION"),
        phase = 10,
        log_dir = %log_dir.display(),
        "DragonForge client starting"
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DragonForge Asset Vault")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([980.0, 640.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    let result = eframe::run_native(
        "DragonForge Asset Vault",
        options,
        Box::new(|_cc| Ok(Box::new(DragonForgeClient::new()))),
    );

    info!("DragonForge client stopped");
    result
}
