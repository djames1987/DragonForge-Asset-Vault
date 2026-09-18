#[path = "../logging.rs"]
mod logging;
mod project_sync;

use project_sync::{EnginePreset, ProjectSyncReport};

use eframe::egui;
use reqwest::{
    blocking::{multipart, Client},
    header::{HeaderMap as ReqwestHeaderMap, HeaderValue as ReqwestHeaderValue},
};
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
    auth_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum UserRole {
    Administrator,
    Developer,
    ReadOnly,
}

impl UserRole {
    fn label(&self) -> &'static str {
        match self {
            Self::Administrator => "Administrator",
            Self::Developer => "Developer",
            Self::ReadOnly => "Read-only",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultUser {
    id: String,
    username: String,
    role: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
    last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthMeResponse {
    enabled: bool,
    authenticated: bool,
    username: Option<String>,
    role: Option<UserRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEvent {
    id: String,
    occurred_at: String,
    actor_user_id: Option<String>,
    actor_username: Option<String>,
    actor_role: Option<String>,
    workstation: Option<String>,
    action: String,
    method: String,
    path: String,
    target_type: Option<String>,
    target_id: Option<String>,
    result: String,
    status_code: i64,
    detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditExportResponse {
    generated_at: String,
    events: Vec<AuditEvent>,
    csv: String,
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
struct AssetCheckout {
    asset_id: String,
    holder: String,
    workstation: String,
    note: Option<String>,
    checked_out_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckoutStatusResponse {
    asset_id: String,
    checkout: Option<AssetCheckout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageTierStatusResponse {
    asset_id: String,
    tier: String,
    archive_enabled: bool,
    object_count: usize,
    transitioned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageTierMoveResponse {
    asset_id: String,
    tier: String,
    objects_moved: usize,
    bytes_moved: u64,
    source_copies_removed: usize,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum AppView {
    Library,
    Projects,
    Activity,
    Backups,
    AiSearch,
    Users,
    Settings,
}

impl AppView {
    fn key(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Projects => "projects",
            Self::Activity => "activity",
            Self::Backups => "backups",
            Self::AiSearch => "ai_search",
            Self::Users => "users",
            Self::Settings => "settings",
        }
    }

    fn from_key(value: &str) -> Self {
        match value {
            "projects" => Self::Projects,
            "activity" => Self::Activity,
            "backups" => Self::Backups,
            "ai_search" => Self::AiSearch,
            "users" => Self::Users,
            "settings" => Self::Settings,
            _ => Self::Library,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Library => "Assets",
            Self::Projects => "Projects",
            Self::Activity => "Activity",
            Self::Backups => "Backups",
            Self::AiSearch => "AI Search",
            Self::Users => "Users",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClientSettings {
    server_url: String,
    #[serde(default)]
    api_token: String,
    #[serde(default = "default_ui_scale")]
    ui_scale: f32,
    #[serde(default = "default_card_width")]
    card_width: f32,
    #[serde(default = "default_sidebar_width")]
    sidebar_width: f32,
    #[serde(default = "default_inspector_width")]
    inspector_width: f32,
    #[serde(default = "default_last_view")]
    last_view: String,
    #[serde(default = "default_true")]
    dark_mode: bool,
    #[serde(default)]
    show_filters: bool,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8080".to_string(),
            api_token: String::new(),
            ui_scale: default_ui_scale(),
            card_width: default_card_width(),
            sidebar_width: default_sidebar_width(),
            inspector_width: default_inspector_width(),
            last_view: default_last_view(),
            dark_mode: true,
            show_filters: false,
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
    AuthMe(Result<AuthMeResponse, String>),
    Users(Result<Vec<VaultUser>, String>),
    UserCreated(Result<VaultUser, String>),
    UserUpdated(Result<VaultUser, String>),
    UserDeleted(Result<String, String>),
    AuditLoaded(Result<Vec<AuditEvent>, String>),
    AuditExported(Result<PathBuf, String>),
    SemanticStatus(Result<SemanticStatusResponse, String>),
    SemanticReindex(Result<ReindexResponse, String>),
    BackupStatus(Result<BackupStatusResponse, String>),
    BackupCreated(Result<BackupCreateResponse, String>),
    BackupVerified(Result<BackupVerifyResponse, String>),
    EnginePresets(Result<Vec<EnginePreset>, String>),
    ProjectSyncChecked(Result<ProjectSyncReport, String>),
    ProjectSyncRepaired(Result<ProjectSyncReport, String>),
    ProjectSyncUpdated(Result<ProjectSyncReport, String>),
    CheckoutStatus {
        asset_id: String,
        result: Result<CheckoutStatusResponse, String>,
    },
    CheckoutChanged {
        asset_id: String,
        result: Result<Option<AssetCheckout>, String>,
    },
    StorageTierStatus {
        asset_id: String,
        result: Result<StorageTierStatusResponse, String>,
    },
    StorageTierChanged {
        asset_id: String,
        result: Result<StorageTierMoveResponse, String>,
    },
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
    current_view: AppView,
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
    engine_presets: Vec<EnginePreset>,
    project_sync_report: Option<ProjectSyncReport>,
    show_project_sync: bool,
    checkout_status: HashMap<String, Option<AssetCheckout>>,
    checkout_note: String,
    storage_tiers: HashMap<String, StorageTierStatusResponse>,
    deleted_only: bool,
    status: String,
    health: Option<HealthResponse>,
    auth_me: Option<AuthMeResponse>,
    users: Vec<VaultUser>,
    show_users: bool,
    new_user_name: String,
    new_user_role: UserRole,
    new_user_token: String,
    selected_user_id: Option<String>,
    selected_user_role: UserRole,
    selected_user_enabled: bool,
    selected_user_token: String,
    show_activity: bool,
    audit_events: Vec<AuditEvent>,
    audit_username: String,
    audit_action: String,
    audit_result: String,
    audit_target_type: String,
    audit_target_id: String,
    audit_from: String,
    audit_to: String,
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

        let current_view = AppView::from_key(&settings.last_view);
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            settings,
            current_view,
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
            engine_presets: Vec::new(),
            project_sync_report: None,
            show_project_sync: false,
            checkout_status: HashMap::new(),
            checkout_note: String::new(),
            storage_tiers: HashMap::new(),
            deleted_only: false,
            status: "Ready".to_string(),
            health: None,
            auth_me: None,
            users: Vec::new(),
            show_users: false,
            new_user_name: String::new(),
            new_user_role: UserRole::Developer,
            new_user_token: String::new(),
            selected_user_id: None,
            selected_user_role: UserRole::Developer,
            selected_user_enabled: true,
            selected_user_token: String::new(),
            show_activity: false,
            audit_events: Vec::new(),
            audit_username: String::new(),
            audit_action: String::new(),
            audit_result: String::new(),
            audit_target_type: String::new(),
            audit_target_id: String::new(),
            audit_from: String::new(),
            audit_to: String::new(),
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
        app.refresh_engine_presets();
        app.refresh_assets();
        app.refresh_projects();
        app
    }

    fn accent() -> egui::Color32 {
        egui::Color32::from_rgb(255, 138, 42)
    }

    fn bg() -> egui::Color32 {
        egui::Color32::from_rgb(11, 14, 20)
    }

    fn panel_bg() -> egui::Color32 {
        egui::Color32::from_rgb(16, 20, 29)
    }

    fn raised_bg() -> egui::Color32 {
        egui::Color32::from_rgb(23, 28, 39)
    }

    fn muted_text() -> egui::Color32 {
        egui::Color32::from_rgb(141, 154, 181)
    }

    fn apply_phase16_style(&self, ctx: &egui::Context) {
        ctx.set_pixels_per_point(self.settings.ui_scale.clamp(0.85, 1.5));

        let mut visuals = if self.settings.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        if self.settings.dark_mode {
            visuals.panel_fill = Self::bg();
            visuals.window_fill = Self::panel_bg();
            visuals.extreme_bg_color = egui::Color32::from_rgb(8, 11, 17);
            visuals.faint_bg_color = Self::panel_bg();
            visuals.widgets.noninteractive.bg_fill = Self::panel_bg();
            visuals.widgets.inactive.bg_fill = Self::raised_bg();
            visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(35, 41, 55);
            visuals.widgets.active.bg_fill = egui::Color32::from_rgb(45, 50, 64);
            visuals.selection.bg_fill = egui::Color32::from_rgb(61, 42, 28);
            visuals.selection.stroke.color = Self::accent();
            visuals.hyperlink_color = Self::accent();
        }

        ctx.set_visuals(visuals);
        ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(14.0, 9.0);
            style.spacing.interact_size.y = 34.0;
        });
    }

    fn navigate(&mut self, view: AppView) {
        self.current_view = view;
        self.settings.last_view = view.key().to_string();
        let _ = save_settings(&self.settings);
        match view {
            AppView::Library => self.refresh_assets(),
            AppView::Projects => self.refresh_projects(),
            AppView::Activity => self.refresh_activity(),
            AppView::Backups => self.refresh_backup_status(),
            AppView::AiSearch => self.refresh_semantic_status(),
            AppView::Users if self.is_admin() => self.refresh_users(),
            _ => {}
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            let logo = egui::Button::new(
                egui::RichText::new("DF")
                    .strong()
                    .color(egui::Color32::BLACK),
            )
            .fill(Self::accent());
            ui.add_sized([44.0, 44.0], logo);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("DragonForge").size(21.0).strong());
                ui.label(egui::RichText::new("Asset Vault").color(Self::muted_text()));
            });
        });

        ui.add_space(20.0);
        if ui
            .add_enabled(
                self.can_write(),
                egui::Button::new(
                    egui::RichText::new("+  Add Asset")
                        .strong()
                        .color(egui::Color32::BLACK),
                )
                .fill(Self::accent())
                .min_size(egui::vec2(ui.available_width(), 42.0)),
            )
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Select an asset to add")
                .pick_file()
            {
                self.begin_upload(path);
            }
        }

        ui.add_space(18.0);
        ui.label(
            egui::RichText::new("LIBRARY")
                .size(10.0)
                .strong()
                .color(Self::muted_text()),
        );

        let nav_button = |ui: &mut egui::Ui, selected: bool, label: &str| {
            let mut button = egui::Button::new(label)
                .min_size(egui::vec2(ui.available_width(), 36.0));
            if selected {
                button = button.fill(egui::Color32::from_rgb(55, 38, 27));
            }
            ui.add(button).clicked()
        };

        if nav_button(
            ui,
            self.current_view == AppView::Library && !self.deleted_only,
            "▦   All Assets",
        ) {
            self.deleted_only = false;
            self.navigate(AppView::Library);
        }

        if nav_button(
            ui,
            self.current_view == AppView::Library && self.deleted_only,
            "♲   Recycle Bin",
        ) {
            self.deleted_only = true;
            self.selected_id = None;
            self.current_view = AppView::Library;
            self.settings.last_view = AppView::Library.key().to_string();
            let _ = save_settings(&self.settings);
            self.refresh_assets();
        }

        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("WORKSPACE")
                .size(10.0)
                .strong()
                .color(Self::muted_text()),
        );

        if nav_button(ui, self.current_view == AppView::Projects, "▣   Projects") {
            self.navigate(AppView::Projects);
        }
        if nav_button(ui, self.current_view == AppView::Activity, "≋   Activity") {
            self.navigate(AppView::Activity);
        }

        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("TOOLS")
                .size(10.0)
                .strong()
                .color(Self::muted_text()),
        );

        if nav_button(ui, self.current_view == AppView::Backups, "◫   Backups") {
            self.navigate(AppView::Backups);
        }
        if nav_button(ui, self.current_view == AppView::AiSearch, "✦   AI Search") {
            self.navigate(AppView::AiSearch);
        }
        if self.is_admin()
            && nav_button(ui, self.current_view == AppView::Users, "♙   Users")
        {
            self.navigate(AppView::Users);
        }
        if nav_button(ui, self.current_view == AppView::Settings, "⚙   Settings") {
            self.navigate(AppView::Settings);
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            egui::Frame::group(ui.style())
                .fill(Self::raised_bg())
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let connected = self.health.as_ref().is_some_and(|health| health.ok);
                    ui.label(
                        egui::RichText::new(if connected {
                            "✓  Server connected"
                        } else {
                            "○  Server offline"
                        })
                        .strong()
                        .color(if connected {
                            egui::Color32::from_rgb(78, 214, 150)
                        } else {
                            Self::muted_text()
                        }),
                    );

                    let user = self
                        .auth_me
                        .as_ref()
                        .and_then(|me| me.username.as_deref())
                        .unwrap_or("Not signed in");
                    let role = self
                        .auth_me
                        .as_ref()
                        .and_then(|me| me.role.as_ref())
                        .map(UserRole::label)
                        .unwrap_or("Offline");
                    ui.small(format!("{user} · {role}"));
                });
            ui.add_space(8.0);
        });
    }

    fn workstation_identity() -> (String, String) {
        let holder = std::env::var("DRAGONFORGE_USER")
            .or_else(|_| std::env::var("USERNAME"))
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "unknown-user".to_string());
        let workstation = std::env::var("DRAGONFORGE_WORKSTATION")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown-workstation".to_string());
        (holder, workstation)
    }

    fn can_write(&self) -> bool {
        if self.health.as_ref().is_some_and(|health| !health.auth_enabled) {
            return true;
        }
        self.auth_me
            .as_ref()
            .and_then(|me| me.role.as_ref())
            .is_some_and(|role| matches!(role, UserRole::Administrator | UserRole::Developer))
    }

    fn is_admin(&self) -> bool {
        if self.health.as_ref().is_some_and(|health| !health.auth_enabled) {
            return true;
        }
        self.auth_me
            .as_ref()
            .and_then(|me| me.role.as_ref())
            .is_some_and(|role| *role == UserRole::Administrator)
    }

    fn audit_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = vec![("limit".to_string(), "500".to_string())];
        for (key, value) in [
            ("username", self.audit_username.trim()),
            ("action", self.audit_action.trim()),
            ("result", self.audit_result.trim()),
            ("target_type", self.audit_target_type.trim()),
            ("target_id", self.audit_target_id.trim()),
            ("from", self.audit_from.trim()),
            ("to", self.audit_to.trim()),
        ] {
            if !value.is_empty() {
                pairs.push((key.to_string(), value.to_string()));
            }
        }
        pairs
    }

    fn checkout_identity(&self) -> (String, String) {
        let (legacy_user, workstation) = Self::workstation_identity();
        let user = self
            .auth_me
            .as_ref()
            .filter(|me| me.enabled && me.authenticated)
            .and_then(|me| me.username.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(legacy_user);
        (user, workstation)
    }

    fn api_client() -> Result<Client, String> {
        let (holder, workstation) = Self::workstation_identity();
        let mut headers = ReqwestHeaderMap::new();
        headers.insert(
            "x-dragonforge-user",
            ReqwestHeaderValue::from_str(&holder.replace(['\r', '\n'], "_"))
                .map_err(|e| e.to_string())?,
        );
        headers.insert(
            "x-dragonforge-workstation",
            ReqwestHeaderValue::from_str(&workstation.replace(['\r', '\n'], "_"))
                .map_err(|e| e.to_string())?,
        );
        let token = std::env::var("DRAGONFORGE_API_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| load_settings().ok().map(|settings| settings.api_token).filter(|value| !value.trim().is_empty()));
        if let Some(token) = token {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                ReqwestHeaderValue::from_str(&format!("Bearer {}", token.trim()))
                    .map_err(|e| e.to_string())?,
            );
        }
        Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .default_headers(headers)
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

    fn refresh_auth_me(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        thread::spawn(move || {
            let result = (|| -> Result<AuthMeResponse, String> {
                Self::api_client()?
                    .get(format!("{base}/api/auth/me"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::AuthMe(result));
        });
    }

    fn refresh_users(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        thread::spawn(move || {
            let result = (|| -> Result<Vec<VaultUser>, String> {
                Self::api_client()?
                    .get(format!("{base}/api/users"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::Users(result));
        });
    }

    fn create_vault_user(&mut self) {
        let username = self.new_user_name.trim().to_string();
        let token = self.new_user_token.trim().to_string();
        if username.is_empty() || token.len() < 16 {
            self.status = "New user needs a username and an API token of at least 16 characters.".to_string();
            return;
        }
        let role = self.new_user_role.clone();
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        thread::spawn(move || {
            let result = (|| -> Result<VaultUser, String> {
                Self::api_client()?
                    .post(format!("{base}/api/users"))
                    .json(&serde_json::json!({"username": username, "role": role, "token": token}))
                    .send().map_err(|e| e.to_string())?
                    .error_for_status().map_err(|e| e.to_string())?
                    .json().map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::UserCreated(result));
        });
    }

    fn save_selected_user(&mut self) {
        let Some(id) = self.selected_user_id.clone() else { return; };
        let role = self.selected_user_role.clone();
        let enabled = self.selected_user_enabled;
        let token = self.selected_user_token.trim().to_string();
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        thread::spawn(move || {
            let body = if token.is_empty() {
                serde_json::json!({"role": role, "enabled": enabled})
            } else {
                serde_json::json!({"role": role, "enabled": enabled, "token": token})
            };
            let result = (|| -> Result<VaultUser, String> {
                Self::api_client()?
                    .patch(format!("{base}/api/users/{id}"))
                    .json(&body)
                    .send().map_err(|e| e.to_string())?
                    .error_for_status().map_err(|e| e.to_string())?
                    .json().map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::UserUpdated(result));
        });
    }

    fn delete_selected_user(&mut self) {
        let Some(id) = self.selected_user_id.clone() else { return; };
        let tx = self.tx.clone();
        let base = self.base_url();
        self.busy_count += 1;
        thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                Self::api_client()?
                    .delete(format!("{base}/api/users/{id}"))
                    .send().map_err(|e| e.to_string())?
                    .error_for_status().map_err(|e| e.to_string())?;
                Ok(id)
            })();
            let _ = tx.send(ClientEvent::UserDeleted(result));
        });
    }

    fn refresh_activity(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        let query = self.audit_query_pairs();
        self.status = "Loading activity history...".to_string();
        thread::spawn(move || {
            let result = (|| -> Result<Vec<AuditEvent>, String> {
                Self::api_client()?
                    .get(format!("{base}/api/audit"))
                    .query(&query)
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::AuditLoaded(result));
        });
    }

    fn export_activity(&mut self) {
        if !self.is_admin() {
            self.status = "Administrator role is required for audit export.".to_string();
            return;
        }
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose audit export folder")
            .pick_folder()
        else {
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let query = self.audit_query_pairs();
        self.busy_count += 1;
        thread::spawn(move || {
            let result = (|| -> Result<PathBuf, String> {
                let response: AuditExportResponse = Self::api_client()?
                    .get(format!("{base}/api/audit/export"))
                    .query(&query)
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())?;
                let stamp = response.generated_at.replace([':', '.'], "-");
                let json_path = folder.join(format!("DragonForge-Audit-{stamp}.json"));
                let csv_path = folder.join(format!("DragonForge-Audit-{stamp}.csv"));
                fs::write(
                    &json_path,
                    serde_json::to_string_pretty(&response.events).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                fs::write(&csv_path, response.csv).map_err(|e| e.to_string())?;
                Ok(json_path)
            })();
            let _ = tx.send(ClientEvent::AuditExported(result));
        });
    }

    fn open_asset_activity(&mut self, asset_id: &str) {
        self.audit_target_type = "asset".to_string();
        self.audit_target_id = asset_id.to_string();
        self.current_view = AppView::Activity;
        self.settings.last_view = AppView::Activity.key().to_string();
        let _ = save_settings(&self.settings);
        self.refresh_activity();
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

    fn refresh_storage_tier(&mut self, asset_id: String) {
        let tx = self.tx.clone();
        let base = self.base_url();
        thread::spawn(move || {
            let result = (|| -> Result<StorageTierStatusResponse, String> {
                Self::api_client()?
                    .get(format!("{base}/api/assets/{asset_id}/storage-tier"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::StorageTierStatus { asset_id, result });
        });
    }

    fn archive_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let asset_id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Archiving {}...", asset.name);
        thread::spawn(move || {
            let result = (|| -> Result<StorageTierMoveResponse, String> {
                Self::api_client()?
                    .post(format!("{base}/api/assets/{asset_id}/archive"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::StorageTierChanged { asset_id, result });
        });
    }

    fn recall_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let asset_id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Recalling {} to hot storage...", asset.name);
        thread::spawn(move || {
            let result = (|| -> Result<StorageTierMoveResponse, String> {
                Self::api_client()?
                    .post(format!("{base}/api/assets/{asset_id}/recall"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::StorageTierChanged { asset_id, result });
        });
    }

    fn refresh_checkout(&mut self, asset_id: String) {
        let tx = self.tx.clone();
        let base = self.base_url();
        thread::spawn(move || {
            let result = (|| -> Result<CheckoutStatusResponse, String> {
                Self::api_client()?
                    .get(format!("{base}/api/assets/{asset_id}/checkout"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::CheckoutStatus { asset_id, result });
        });
    }

    fn checkout_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let (holder, workstation) = self.checkout_identity();
        let body = serde_json::json!({
            "holder": holder,
            "workstation": workstation,
            "note": if self.checkout_note.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(self.checkout_note.trim().to_string())
            }
        });
        let tx = self.tx.clone();
        let base = self.base_url();
        let asset_id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Checking out {}...", asset.name);
        thread::spawn(move || {
            let result = (|| -> Result<Option<AssetCheckout>, String> {
                let checkout: AssetCheckout = Self::api_client()?
                    .post(format!("{base}/api/assets/{asset_id}/checkout"))
                    .json(&body)
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())?;
                Ok(Some(checkout))
            })();
            let _ = tx.send(ClientEvent::CheckoutChanged { asset_id, result });
        });
    }

    fn checkin_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            self.status = "Select an asset first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let asset_id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Checking in {}...", asset.name);
        thread::spawn(move || {
            let result = (|| -> Result<Option<AssetCheckout>, String> {
                Self::api_client()?
                    .delete(format!("{base}/api/assets/{asset_id}/checkout"))
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?;
                Ok(None)
            })();
            let _ = tx.send(ClientEvent::CheckoutChanged { asset_id, result });
        });
    }

    fn refresh_engine_presets(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        thread::spawn(move || {
            let result = project_sync::fetch_engine_presets(&base);
            let _ = tx.send(ClientEvent::EnginePresets(result));
        });
    }

    fn check_selected_project_sync(&mut self) {
        let Some(project) = self.selected_project().cloned() else {
            self.status = "Select a project first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let presets = self.engine_presets.clone();
        self.busy_count += 1;
        self.status = format!("Checking project sync for {}...", project.name);
        thread::spawn(move || {
            let result = project_sync::check_project(&base, &project, &presets);
            let _ = tx.send(ClientEvent::ProjectSyncChecked(result));
        });
    }

    fn repair_selected_project_sync(&mut self) {
        let Some(project) = self.selected_project().cloned() else {
            self.status = "Select a project first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let presets = self.engine_presets.clone();
        self.busy_count += 1;
        self.status = format!("Repairing pinned project files for {}...", project.name);
        thread::spawn(move || {
            let result = project_sync::repair_project(&base, &project, &presets);
            let _ = tx.send(ClientEvent::ProjectSyncRepaired(result));
        });
    }

    fn update_selected_project_to_latest(&mut self) {
        let Some(project) = self.selected_project().cloned() else {
            self.status = "Select a project first.".to_string();
            return;
        };
        let tx = self.tx.clone();
        let base = self.base_url();
        let presets = self.engine_presets.clone();
        self.busy_count += 1;
        self.status = format!("Updating {} to latest vault revisions...", project.name);
        thread::spawn(move || {
            let result = project_sync::update_project_to_latest(&base, &project, &presets);
            let _ = tx.send(ClientEvent::ProjectSyncUpdated(result));
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
            let result = project_sync::add_current_asset(&base, &project, &asset);
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
            let result = project_sync::remove_asset(&base, &project, &asset).map(|message| {
                match write_project_license_files(&base, &project) {
                    Ok((_path, warnings, unknown)) => format!(
                        "{message} License files refreshed ({warnings} warnings, {unknown} unknown)."
                    ),
                    Err(err) => format!(
                        "{message} Warning: license files could not be refreshed automatically: {err}"
                    ),
                }
            });
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
                            info!(phase = health.phase, version = %health.version, auth_enabled = health.auth_enabled, "server connection successful");
                            self.status = format!("Connected to {} v{}", health.service, health.version);
                            let auth_enabled = health.auth_enabled;
                            self.health = Some(health);
                            if auth_enabled {
                                self.refresh_auth_me();
                            } else {
                                self.auth_me = Some(AuthMeResponse {
                                    enabled: false,
                                    authenticated: true,
                                    username: None,
                                    role: Some(UserRole::Administrator),
                                });
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "server connection failed");
                            self.health = None;
                            self.status = format!("Connection failed: {err}");
                        }
                    }
                }
                ClientEvent::AuthMe(result) => {
                    match result {
                        Ok(me) => {
                            info!(username = ?me.username, role = ?me.role, "authentication status loaded");
                            self.auth_me = Some(me);
                        }
                        Err(err) => {
                            warn!(error = %err, "authentication failed");
                            self.auth_me = None;
                            self.status = format!("Authentication failed: {err}");
                        }
                    }
                }
                ClientEvent::Users(result) => {
                    match result {
                        Ok(users) => {
                            self.users = users;
                            if let Some(id) = self.selected_user_id.clone() {
                                if let Some(user) = self.users.iter().find(|user| user.id == id) {
                                    self.selected_user_enabled = user.enabled;
                                    self.selected_user_role = match user.role.as_str() {
                                        "administrator" => UserRole::Administrator,
                                        "read_only" => UserRole::ReadOnly,
                                        _ => UserRole::Developer,
                                    };
                                }
                            }
                        }
                        Err(err) => self.status = format!("User list failed: {err}"),
                    }
                }
                ClientEvent::UserCreated(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(user) => {
                            self.status = format!("Created user {}", user.username);
                            self.new_user_name.clear();
                            self.new_user_token.clear();
                            self.refresh_users();
                        }
                        Err(err) => self.status = format!("Create user failed: {err}"),
                    }
                }
                ClientEvent::UserUpdated(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(user) => {
                            self.status = format!("Updated user {}", user.username);
                            self.selected_user_token.clear();
                            self.refresh_users();
                            self.refresh_auth_me();
                        }
                        Err(err) => self.status = format!("Update user failed: {err}"),
                    }
                }
                ClientEvent::UserDeleted(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(id) => {
                            self.status = "User deleted".to_string();
                            if self.selected_user_id.as_deref() == Some(id.as_str()) {
                                self.selected_user_id = None;
                            }
                            self.refresh_users();
                        }
                        Err(err) => self.status = format!("Delete user failed: {err}"),
                    }
                }
                ClientEvent::AuditLoaded(result) => {
                    match result {
                        Ok(events) => {
                            self.status = format!("{} activity events loaded", events.len());
                            self.audit_events = events;
                        }
                        Err(err) => self.status = format!("Activity load failed: {err}"),
                    }
                }
                ClientEvent::AuditExported(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(path) => self.status = format!("Audit JSON/CSV exported beside {}", path.display()),
                        Err(err) => self.status = format!("Audit export failed: {err}"),
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
                ClientEvent::EnginePresets(result) => {
                    match result {
                        Ok(presets) => {
                            info!(count = presets.len(), "engine presets loaded");
                            self.engine_presets = presets;
                        }
                        Err(err) => {
                            warn!(error = %err, "engine presets unavailable");
                            self.status = format!("Engine presets unavailable: {err}");
                        }
                    }
                }
                ClientEvent::ProjectSyncChecked(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(report) => {
                            info!(
                                project_id = %report.project_id,
                                engine = %report.engine,
                                assets = report.items.len(),
                                issues = report.issue_count(),
                                "project sync checked"
                            );
                            self.status = format!(
                                "Project sync: {}/{} assets clean · {} issues",
                                report.in_sync_count(),
                                report.items.len(),
                                report.issue_count()
                            );
                            self.project_sync_report = Some(report);
                            self.show_project_sync = true;
                        }
                        Err(err) => {
                            warn!(error = %err, "project sync check failed");
                            self.status = format!("Project sync check failed: {err}");
                        }
                    }
                }
                ClientEvent::ProjectSyncRepaired(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(report) => {
                            info!(
                                project_id = %report.project_id,
                                assets = report.items.len(),
                                issues = report.issue_count(),
                                "project pinned files repaired"
                            );
                            self.status = format!(
                                "Pinned project files repaired · {} issues remain",
                                report.issue_count()
                            );
                            self.project_sync_report = Some(report);
                            self.show_project_sync = true;
                            if let Some(project) = self.selected_project().cloned() {
                                let _ = write_project_license_files(&self.base_url(), &project);
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "project sync repair failed");
                            self.status = format!("Project repair failed: {err}");
                        }
                    }
                }
                ClientEvent::ProjectSyncUpdated(result) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(report) => {
                            info!(
                                project_id = %report.project_id,
                                assets = report.items.len(),
                                issues = report.issue_count(),
                                "project updated to latest revisions"
                            );
                            self.status = format!(
                                "Project updated to latest revisions · {} issues remain",
                                report.issue_count()
                            );
                            self.project_sync_report = Some(report);
                            self.show_project_sync = true;
                            if let Some(project) = self.selected_project().cloned() {
                                let _ = write_project_license_files(&self.base_url(), &project);
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "project update to latest failed");
                            self.status = format!("Project update failed: {err}");
                        }
                    }
                }
                ClientEvent::StorageTierStatus { asset_id, result } => {
                    match result {
                        Ok(status) => {
                            self.storage_tiers.insert(asset_id, status);
                        }
                        Err(err) => {
                            warn!(asset_id = %asset_id, error = %err, "storage tier status failed");
                            self.status = format!("Storage tier status failed: {err}");
                        }
                    }
                }
                ClientEvent::StorageTierChanged { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(response) => {
                            info!(
                                asset_id = %asset_id,
                                tier = %response.tier,
                                objects_moved = response.objects_moved,
                                bytes_moved = response.bytes_moved,
                                source_copies_removed = response.source_copies_removed,
                                "asset storage tier changed"
                            );
                            self.status = format!(
                                "Storage tier: {} · {} objects · {} moved · {} source copies removed",
                                response.tier,
                                response.objects_moved,
                                human_size(response.bytes_moved as i64),
                                response.source_copies_removed
                            );
                            self.refresh_storage_tier(asset_id);
                        }
                        Err(err) => {
                            warn!(asset_id = %asset_id, error = %err, "storage tier operation failed");
                            self.status = format!("Storage tier operation failed: {err}");
                            self.refresh_storage_tier(asset_id);
                        }
                    }
                }
                ClientEvent::CheckoutStatus { asset_id, result } => {
                    match result {
                        Ok(response) => {
                            self.checkout_status.insert(asset_id, response.checkout);
                        }
                        Err(err) => {
                            warn!(asset_id = %asset_id, error = %err, "checkout status failed");
                            self.status = format!("Checkout status failed: {err}");
                        }
                    }
                }
                ClientEvent::CheckoutChanged { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(checkout) => {
                            let message = if let Some(lock) = &checkout {
                                info!(
                                    asset_id = %asset_id,
                                    holder = %lock.holder,
                                    workstation = %lock.workstation,
                                    "asset checked out in client"
                                );
                                format!("Checked out to {}@{}", lock.holder, lock.workstation)
                            } else {
                                info!(asset_id = %asset_id, "asset checked in from client");
                                "Asset checked in".to_string()
                            };
                            self.checkout_status.insert(asset_id, checkout);
                            self.checkout_note.clear();
                            self.status = message;
                        }
                        Err(err) => {
                            warn!(asset_id = %asset_id, error = %err, "checkout operation failed");
                            self.status = format!("Checkout operation failed: {err}");
                            self.refresh_checkout(asset_id);
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
        if self.current_view == AppView::Library {
            return;
        }

        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new(self.current_view.label()).size(23.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.busy_count > 0 {
                    ui.spinner();
                }
                if ui.button("Refresh").clicked() {
                    match self.current_view {
                        AppView::Projects => self.refresh_projects(),
                        AppView::Activity => self.refresh_activity(),
                        AppView::Backups => self.refresh_backup_status(),
                        AppView::AiSearch => self.refresh_semantic_status(),
                        AppView::Users => self.refresh_users(),
                        AppView::Settings => self.check_server(),
                        AppView::Library => {}
                    }
                }
            });
        });
    }

    fn filter_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new(if self.deleted_only {
                    "Recycle Bin"
                } else {
                    "All Assets"
                })
                .size(22.0)
                .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.can_write(),
                        egui::Button::new(
                            egui::RichText::new("+")
                                .size(20.0)
                                .color(Self::accent()),
                        ),
                    )
                    .on_hover_text("Import a multi-file package ZIP")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Select a multi-file asset ZIP")
                        .add_filter("ZIP package", &["zip"])
                        .pick_file()
                    {
                        self.begin_package_import(path);
                    }
                }
            });
        });
        ui.label(
            egui::RichText::new(format!("{} assets", self.assets.len()))
                .color(Self::muted_text()),
        );
        ui.add_space(10.0);

        let search = ui.add_sized(
            [ui.available_width(), 42.0],
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("⌕   Search your asset vault"),
        );
        if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.refresh_assets();
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("phase161_search_mode")
                .selected_text(format!("{} Search", self.search_mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.search_mode, "Smart".to_string(), "Smart Search");
                    ui.selectable_value(&mut self.search_mode, "Keyword".to_string(), "Keyword Search");
                });

            if ui
                .selectable_label(self.settings.show_filters, "Filters")
                .clicked()
            {
                self.settings.show_filters = !self.settings.show_filters;
                let _ = save_settings(&self.settings);
            }
            if ui.button("Search").clicked() {
                self.refresh_assets();
            }
        });

        if self.settings.show_filters {
            ui.add_space(8.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("Category");
                ui.add(egui::TextEdit::singleline(&mut self.category_filter).hint_text("e.g. texture"));
                ui.label("Tag");
                ui.add(egui::TextEdit::singleline(&mut self.tag_filter).hint_text("e.g. wood"));
                ui.horizontal(|ui| {
                    ui.label("Type");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.extension_filter)
                            .desired_width(70.0)
                            .hint_text("obj"),
                    );
                    egui::ComboBox::from_id_salt("phase161_license")
                        .selected_text(&self.license_filter)
                        .show_ui(ui, |ui| {
                            for value in ["All", "Complete", "Warning", "Unknown", "Custom"] {
                                ui.selectable_value(&mut self.license_filter, value.to_string(), value);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        self.refresh_assets();
                    }
                    if ui.button("Clear").clicked() {
                        self.category_filter.clear();
                        self.tag_filter.clear();
                        self.extension_filter.clear();
                        self.license_filter = "All".to_string();
                        self.refresh_assets();
                    }
                });
            });
        }
    }

    fn asset_grid(&mut self, ui: &mut egui::Ui) {
        let mut chosen: Option<String> = None;

        egui::ScrollArea::vertical()
            .id_salt("phase161_asset_list")
            .show(ui, |ui| {
                for asset in &self.assets {
                    let selected = self.selected_id.as_deref() == Some(asset.id.as_str());
                    let fill = if selected {
                        egui::Color32::from_rgb(43, 34, 29)
                    } else {
                        Self::panel_bg()
                    };

                    let response = egui::Frame::group(ui.style())
                        .fill(fill)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            ui.horizontal(|ui| {
                                let thumb_size = egui::vec2(58.0, 58.0);
                                if let Some(texture) = self.thumbnails.get(&asset.id) {
                                    ui.image((texture.id(), thumb_size));
                                } else {
                                    let ext = asset
                                        .extension
                                        .as_deref()
                                        .unwrap_or("FILE")
                                        .to_ascii_uppercase();
                                    ui.add_sized(
                                        thumb_size,
                                        egui::Label::new(
                                            egui::RichText::new(ext).strong().color(Self::muted_text()),
                                        ),
                                    );
                                }

                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(&asset.name)
                                            .size(15.0)
                                            .strong()
                                            .color(if selected {
                                                Self::accent()
                                            } else {
                                                egui::Color32::WHITE
                                            }),
                                    );

                                    let ext = asset
                                        .extension
                                        .as_deref()
                                        .unwrap_or("file")
                                        .to_ascii_uppercase();
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} · {} · v{}",
                                            ext,
                                            human_size(asset.byte_size),
                                            asset.current_version
                                        ))
                                        .color(Self::muted_text()),
                                    );

                                    let mut metadata = asset
                                        .category
                                        .clone()
                                        .unwrap_or_else(|| "Uncategorized".to_string());
                                    if let Some(Some(checkout)) = self.checkout_status.get(&asset.id) {
                                        metadata.push_str(&format!(" · checked out by {}", checkout.holder));
                                    }
                                    if self
                                        .storage_tiers
                                        .get(&asset.id)
                                        .is_some_and(|status| status.tier == "archive")
                                    {
                                        metadata.push_str(" · ARCHIVED");
                                    }
                                    ui.small(metadata);
                                });
                            });
                        })
                        .response
                        .interact(egui::Sense::click());

                    if response.clicked() {
                        chosen = Some(asset.id.clone());
                    }
                    ui.add_space(6.0);
                }
            });

        if let Some(id) = chosen {
            info!(asset_id = %id, "asset selected");
            self.selected_id = Some(id.clone());
            self.refresh_checkout(id.clone());
            self.refresh_storage_tier(id);
        }
    }

    fn details_panel(&mut self, ui: &mut egui::Ui) {
        let can_write = self.can_write();

        let Some(asset) = self.selected_asset().cloned() else {
            ui.with_layout(
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.add_space(180.0);
                    ui.label(
                        egui::RichText::new("⌘")
                            .size(44.0)
                            .color(egui::Color32::from_rgb(75, 92, 126)),
                    );
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Select an asset").size(22.0).strong());
                    ui.label(
                        egui::RichText::new(
                            "Choose something from your asset vault to inspect its details and preview.",
                        )
                        .color(Self::muted_text()),
                    );
                },
            );
            return;
        };

        egui::ScrollArea::vertical()
            .id_salt("phase161_detail_scroll")
            .show(ui, |ui| {
                ui.heading(egui::RichText::new(&asset.name).size(25.0).strong());
                ui.label(
                    egui::RichText::new(&asset.original_filename)
                        .color(Self::muted_text()),
                );
                ui.add_space(12.0);

                if let Some(texture) = self.thumbnails.get(&asset.id) {
                    let max_width = ui.available_width().min(720.0);
                    let size = egui::vec2(max_width, (max_width * 0.55).min(410.0));
                    ui.image((texture.id(), size));
                } else {
                    egui::Frame::group(ui.style())
                        .fill(Self::panel_bg())
                        .show(ui, |ui| {
                            ui.set_min_size(egui::vec2(ui.available_width(), 260.0));
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new(
                                        asset.extension.as_deref().unwrap_or("ASSET").to_ascii_uppercase(),
                                    )
                                    .size(28.0)
                                    .strong()
                                    .color(Self::muted_text()),
                                );
                            });
                        });
                }

                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Download").clicked() {
                        self.download_selected();
                    }

                    let (identity_user, identity_workstation) = self.checkout_identity();
                    match self.checkout_status.get(&asset.id) {
                        Some(Some(checkout))
                            if checkout.holder == identity_user
                                && checkout.workstation == identity_workstation =>
                        {
                            if ui
                                .add_enabled(can_write, egui::Button::new("Check In"))
                                .clicked()
                            {
                                self.checkin_selected();
                            }
                        }
                        Some(None) => {
                            if ui
                                .add_enabled(
                                    can_write && asset.deleted_at.is_none(),
                                    egui::Button::new("Check Out").fill(Self::accent()),
                                )
                                .clicked()
                            {
                                self.checkout_selected();
                            }
                        }
                        _ => {}
                    }

                    if ui
                        .add_enabled(can_write, egui::Button::new("Edit Metadata"))
                        .clicked()
                    {
                        self.begin_edit_selected();
                    }
                    if ui.button("Version History").clicked() {
                        self.open_versions_selected();
                    }
                    if ui.button("Activity").clicked() {
                        self.open_asset_activity(&asset.id);
                    }
                });

                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new("GENERAL")
                        .size(11.0)
                        .strong()
                        .color(Self::muted_text()),
                );
                ui.separator();

                egui::Grid::new("phase161_general")
                    .num_columns(2)
                    .spacing([26.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Category").color(Self::muted_text()));
                        ui.label(asset.category.as_deref().unwrap_or("Uncategorized"));
                        ui.end_row();

                        ui.label(egui::RichText::new("Creator").color(Self::muted_text()));
                        ui.label(asset.creator.as_deref().unwrap_or("Unknown"));
                        ui.end_row();

                        ui.label(egui::RichText::new("Type").color(Self::muted_text()));
                        ui.label(asset.extension.as_deref().unwrap_or("unknown").to_ascii_uppercase());
                        ui.end_row();

                        ui.label(egui::RichText::new("Size").color(Self::muted_text()));
                        ui.label(human_size(asset.byte_size));
                        ui.end_row();

                        ui.label(egui::RichText::new("Version").color(Self::muted_text()));
                        ui.label(format!("v{}", asset.current_version));
                        ui.end_row();

                        ui.label(egui::RichText::new("License").color(Self::muted_text()));
                        ui.label(asset.license.as_deref().unwrap_or("Not recorded"));
                        ui.end_row();
                    });

                if let Some(description) = asset.description.as_deref().filter(|v| !v.is_empty()) {
                    ui.add_space(12.0);
                    ui.label(description);
                }

                if !asset.tags.is_empty() {
                    ui.add_space(18.0);
                    ui.label(
                        egui::RichText::new("TAGS")
                            .size(11.0)
                            .strong()
                            .color(Self::muted_text()),
                    );
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        for tag in &asset.tags {
                            ui.label(
                                egui::RichText::new(format!("#{tag}"))
                                    .color(Self::accent()),
                            );
                        }
                    });
                }

                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new("COLLABORATION")
                        .size(11.0)
                        .strong()
                        .color(Self::muted_text()),
                );
                ui.separator();

                match self.checkout_status.get(&asset.id) {
                    Some(Some(checkout)) => {
                        ui.label(format!(
                            "● Checked out by {}@{}",
                            checkout.holder, checkout.workstation
                        ));
                        ui.small(format!("Since {}", checkout.checked_out_at));
                    }
                    Some(None) => {
                        ui.label("✓ Available");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.checkout_note)
                                .hint_text("Optional checkout note")
                                .desired_width(360.0),
                        );
                    }
                    None => {
                        if ui.button("Load checkout status").clicked() {
                            self.refresh_checkout(asset.id.clone());
                        }
                    }
                }

                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new("STORAGE")
                        .size(11.0)
                        .strong()
                        .color(Self::muted_text()),
                );
                ui.separator();

                match self.storage_tiers.get(&asset.id).cloned() {
                    Some(status) => {
                        ui.label(format!(
                            "● {} · {} object(s)",
                            status.tier.to_ascii_uppercase(),
                            status.object_count
                        ));
                        if status.archive_enabled {
                            if status.tier == "archive" {
                                if ui
                                    .add_enabled(
                                        can_write && self.busy_count == 0,
                                        egui::Button::new("Recall to Hot Storage"),
                                    )
                                    .clicked()
                                {
                                    self.recall_selected();
                                }
                            } else if ui
                                .add_enabled(
                                    can_write && self.busy_count == 0 && asset.deleted_at.is_none(),
                                    egui::Button::new("Archive"),
                                )
                                .clicked()
                            {
                                self.archive_selected();
                            }
                        }
                    }
                    None => {
                        if ui.button("Load storage status").clicked() {
                            self.refresh_storage_tier(asset.id.clone());
                        }
                    }
                }

                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new("PROJECT")
                        .size(11.0)
                        .strong()
                        .color(Self::muted_text()),
                );
                ui.separator();

                let project_name = self
                    .selected_project()
                    .map(|project| project.name.clone())
                    .unwrap_or_else(|| "No project selected".to_string());
                ui.label(project_name);

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            can_write
                                && self.selected_project_id.is_some()
                                && asset.deleted_at.is_none(),
                            egui::Button::new("Add to Project"),
                        )
                        .clicked()
                    {
                        self.add_selected_to_project();
                    }
                    if ui
                        .add_enabled(
                            can_write && self.selected_project_id.is_some(),
                            egui::Button::new("Remove from Project"),
                        )
                        .clicked()
                    {
                        self.request_remove_selected_from_project();
                    }
                });

                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new("MORE")
                        .size(11.0)
                        .strong()
                        .color(Self::muted_text()),
                );
                ui.separator();

                ui.horizontal_wrapped(|ui| {
                    if ui.button("Package / Dependencies").clicked() {
                        self.open_package_contents();
                    }
                    if asset.deleted_at.is_some() {
                        if ui
                            .add_enabled(can_write, egui::Button::new("Restore Asset"))
                            .clicked()
                        {
                            self.restore_selected();
                        }
                    } else if ui
                        .add_enabled(can_write, egui::Button::new("Move to Recycle Bin"))
                        .clicked()
                    {
                        self.delete_selected();
                    }
                });
            });
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
                    .add_enabled(self.busy_count == 0 && self.can_write(), egui::Button::new("Upload to Vault"))
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
                    .add_enabled(self.busy_count == 0 && self.can_write(), egui::Button::new("Save Changes"))
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
                    if ui.add_enabled(self.can_write(), egui::Button::new("Upload New Version")).clicked() {
                        self.upload_new_version();
                    }
                    if ui.add_enabled(self.can_write(), egui::Button::new("Upload Package Version ZIP")).clicked() {
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
                                && ui.add_enabled(
                                    self.can_write(),
                                    egui::Button::new(format!("Restore v{} as New Current Version", version.version_number)),
                                ).clicked()
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
                if ui.add_enabled(self.busy_count == 0 && self.can_write(), egui::Button::new("Import Package")).clicked() {
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
                        .add_enabled(self.busy_count == 0 && self.can_write(), egui::Button::new("Remove from Project"))
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

    fn projects_page(&mut self, ui: &mut egui::Ui) {
        let can_write = self.can_write();
        ui.horizontal(|ui| {
            ui.heading("Projects");
            if ui.add_enabled(can_write, egui::Button::new("+ New Project")).clicked() {
                self.show_project_create = true;
            }
            if ui.button("Refresh").clicked() {
                self.refresh_projects();
            }
        });
        ui.label("Engine-aware project exports, license manifests, and sync health.");
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for project in self.projects.clone() {
                let selected = self.selected_project_id.as_deref() == Some(project.id.as_str());
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(selected, &project.name).clicked() {
                            self.selected_project_id = Some(project.id.clone());
                        }
                        ui.label(format!("{} · {}", project.engine, project.local_path));
                    });
                    if let Some(description) = project.description.as_deref().filter(|v| !v.is_empty()) {
                        ui.small(description);
                    }
                    if selected {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Check Sync").clicked() {
                                self.check_selected_project_sync();
                            }
                            if ui.add_enabled(can_write, egui::Button::new("Repair Pinned")).clicked() {
                                self.repair_selected_project_sync();
                            }
                            if ui.add_enabled(can_write, egui::Button::new("Update to Latest")).clicked() {
                                self.update_selected_project_to_latest();
                            }
                            if ui.button("Refresh Credits").clicked() {
                                self.refresh_project_license_files();
                            }
                        });
                    }
                });
                ui.add_space(8.0);
            }
        });
    }

    fn activity_page(&mut self, ui: &mut egui::Ui) {
        let is_admin = self.is_admin();
        ui.horizontal_wrapped(|ui| {
            ui.label("User");
            ui.add(egui::TextEdit::singleline(&mut self.audit_username).desired_width(110.0));
            ui.label("Action");
            ui.add(egui::TextEdit::singleline(&mut self.audit_action).desired_width(140.0));
            ui.label("Result");
            egui::ComboBox::from_id_salt("phase16_audit_result")
                .selected_text(if self.audit_result.is_empty() { "All" } else { &self.audit_result })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.audit_result, String::new(), "All");
                    ui.selectable_value(&mut self.audit_result, "success".to_string(), "Success");
                    ui.selectable_value(&mut self.audit_result, "failure".to_string(), "Failure");
                });
            if ui.button("Apply").clicked() {
                self.refresh_activity();
            }
            if is_admin && ui.button("Export JSON + CSV").clicked() {
                self.export_activity();
            }
            if ui.button("Clear").clicked() {
                self.audit_username.clear();
                self.audit_action.clear();
                self.audit_result.clear();
                self.audit_target_type.clear();
                self.audit_target_id.clear();
                self.audit_from.clear();
                self.audit_to.clear();
                self.refresh_activity();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Target");
            ui.add(egui::TextEdit::singleline(&mut self.audit_target_type).desired_width(90.0).hint_text("asset"));
            ui.add(egui::TextEdit::singleline(&mut self.audit_target_id).desired_width(190.0).hint_text("target id"));
            ui.label("From");
            ui.add(egui::TextEdit::singleline(&mut self.audit_from).desired_width(170.0).hint_text("RFC3339"));
            ui.label("To");
            ui.add(egui::TextEdit::singleline(&mut self.audit_to).desired_width(170.0).hint_text("RFC3339"));
        });
        ui.separator();
        ui.label(format!("{} events", self.audit_events.len()));
        egui::ScrollArea::vertical().show(ui, |ui| {
            for event in &self.audit_events {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(&event.action);
                        ui.label(format!("{} · HTTP {}", event.result.to_ascii_uppercase(), event.status_code));
                        ui.label(&event.occurred_at);
                    });
                    ui.small(format!(
                        "{} ({}) @ {}",
                        event.actor_username.as_deref().unwrap_or("anonymous"),
                        event.actor_role.as_deref().unwrap_or("unknown"),
                        event.workstation.as_deref().unwrap_or("unknown")
                    ));
                    ui.small(format!("{} {}", event.method, event.path));
                    if let Some(kind) = event.target_type.as_deref() {
                        ui.small(format!("Target: {} {}", kind, event.target_id.as_deref().unwrap_or("")));
                    }
                });
                ui.add_space(6.0);
            }
        });
    }

    fn backups_page(&mut self, ui: &mut egui::Ui) {
        let is_admin = self.is_admin();
        ui.horizontal(|ui| {
            if ui.add_enabled(is_admin && self.busy_count == 0, egui::Button::new("Create Backup")).clicked() {
                self.create_vault_backup();
            }
            if ui.add_enabled(
                is_admin && self.busy_count == 0 && self.backup_status.as_ref().is_some_and(|s| !s.backups.is_empty()),
                egui::Button::new("Verify Latest"),
            ).clicked() {
                self.verify_latest_backup();
            }
            if ui.button("Refresh").clicked() {
                self.refresh_backup_status();
            }
        });
        ui.separator();
        match self.backup_status.clone() {
            Some(status) => {
                ui.label(format!("Backup directory: {}", status.backup_directory));
                ui.label(format!("Retention: {} snapshots", status.keep));
                if !status.replication_targets.is_empty() {
                    ui.label(format!("Replication: {}", status.replication_targets.join(", ")));
                }
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for backup in status.backups {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.strong(&backup.backup_id);
                                ui.label(human_size(backup.total_bytes as i64));
                                ui.label(format!("{} files", backup.files));
                                ui.label(if backup.verified { "VERIFIED" } else { "NOT VERIFIED" });
                            });
                            ui.small(backup.created_at);
                        });
                        ui.add_space(6.0);
                    }
                });
            }
            None => {
                ui.label("Backup status has not been loaded.");
            }
        }
    }

    fn ai_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Local AI Search");
        ui.label("Semantic search runs locally through Ollama and falls back to keyword search when unavailable.");
        ui.separator();
        if let Some(status) = self.semantic_status.clone() {
            ui.label(format!("Enabled: {}", status.enabled));
            ui.label(format!("Ollama reachable: {}", status.ollama_reachable));
            ui.label(format!("Model: {}", status.model));
            ui.label(format!("Indexed: {} / {}", status.indexed_assets, status.total_active_assets));
            ui.label(format!("Stale: {}", status.stale_assets));
            if let Some(error) = status.last_error {
                ui.label(format!("Last error: {error}"));
            }
        } else {
            ui.label("AI status has not been loaded.");
        }
        ui.add_space(8.0);
        if ui.add_enabled(self.can_write(), egui::Button::new("Reindex AI Search")).clicked() {
            self.reindex_semantic_search();
        }
        if ui.button("Refresh AI Status").clicked() {
            self.refresh_semantic_status();
        }
    }

    fn users_page(&mut self, ui: &mut egui::Ui) {
        if !self.is_admin() {
            ui.label("Administrator role is required.");
            return;
        }

        ui.heading("User Management");
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Username");
                ui.add(egui::TextEdit::singleline(&mut self.new_user_name).desired_width(140.0));
                egui::ComboBox::from_id_salt("phase16_new_user_role")
                    .selected_text(self.new_user_role.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.new_user_role, UserRole::Administrator, "Administrator");
                        ui.selectable_value(&mut self.new_user_role, UserRole::Developer, "Developer");
                        ui.selectable_value(&mut self.new_user_role, UserRole::ReadOnly, "Read-only");
                    });
                ui.label("Token");
                ui.add(egui::TextEdit::singleline(&mut self.new_user_token).password(true).desired_width(220.0));
                if ui.add_enabled(self.busy_count == 0, egui::Button::new("Create User")).clicked() {
                    self.create_vault_user();
                }
            });
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for user in self.users.clone() {
                let selected = self.selected_user_id.as_deref() == Some(user.id.as_str());
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    if ui.selectable_label(
                        selected,
                        format!("{} · {} · {}", user.username, user.role, if user.enabled { "enabled" } else { "disabled" }),
                    ).clicked() {
                        self.selected_user_id = Some(user.id.clone());
                        self.selected_user_enabled = user.enabled;
                        self.selected_user_role = match user.role.as_str() {
                            "administrator" => UserRole::Administrator,
                            "read_only" => UserRole::ReadOnly,
                            _ => UserRole::Developer,
                        };
                        self.selected_user_token.clear();
                    }
                    if selected {
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut self.selected_user_enabled, "Enabled");
                            egui::ComboBox::from_id_salt(format!("phase16_role_{}", user.id))
                                .selected_text(self.selected_user_role.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.selected_user_role, UserRole::Administrator, "Administrator");
                                    ui.selectable_value(&mut self.selected_user_role, UserRole::Developer, "Developer");
                                    ui.selectable_value(&mut self.selected_user_role, UserRole::ReadOnly, "Read-only");
                                });
                            ui.add(egui::TextEdit::singleline(&mut self.selected_user_token).password(true).hint_text("New token (optional)").desired_width(190.0));
                            if ui.button("Save").clicked() {
                                self.save_selected_user();
                            }
                            if ui.button("Delete").clicked() {
                                self.delete_selected_user();
                            }
                        });
                    }
                });
                ui.add_space(6.0);
            }
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Connection");
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Server URL");
            ui.add(egui::TextEdit::singleline(&mut self.settings.server_url).desired_width(420.0));
            ui.label("API Token");
            ui.add(egui::TextEdit::singleline(&mut self.settings.api_token).password(true).desired_width(420.0));
            if ui.button("Save & Connect").clicked() {
                let _ = save_settings(&self.settings);
                self.check_server();
                self.refresh_auth_me();
                self.refresh_assets();
                self.refresh_projects();
            }
        });

        ui.add_space(12.0);
        ui.heading("Appearance");
        egui::Frame::group(ui.style()).show(ui, |ui| {
            let mut changed = false;
            changed |= ui.checkbox(&mut self.settings.dark_mode, "Dark mode").changed();
            changed |= ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.85..=1.5).text("UI scale")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.settings.card_width, 170.0..=300.0).text("Asset list density")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.settings.sidebar_width, 170.0..=280.0).text("Sidebar width")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.settings.inspector_width, 280.0..=480.0).text("Inspector width")).changed();
            if changed {
                let _ = save_settings(&self.settings);
                self.apply_phase16_style(ctx);
            }
        });

        ui.add_space(12.0);
        ui.heading("Client");
        let (_, workstation) = Self::workstation_identity();
        ui.label(format!("Workstation: {workstation}"));
        ui.label(format!("Logs: {}", client_log_dir().display()));
        if let Some(health) = &self.health {
            ui.label(format!("Server phase {} · v{}", health.phase, health.version));
        }
    }

    fn activity_window(&mut self, ctx: &egui::Context) {
        if !self.show_activity {
            return;
        }
        let mut open = self.show_activity;
        let is_admin = self.is_admin();
        egui::Window::new("DragonForge Activity")
            .open(&mut open)
            .resizable(true)
            .default_width(980.0)
            .default_height(650.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("User");
                    ui.add(egui::TextEdit::singleline(&mut self.audit_username).desired_width(100.0));
                    ui.label("Action");
                    ui.add(egui::TextEdit::singleline(&mut self.audit_action).desired_width(130.0));
                    ui.label("Result");
                    egui::ComboBox::from_id_salt("audit_result")
                        .selected_text(if self.audit_result.is_empty() { "All" } else { &self.audit_result })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.audit_result, String::new(), "All");
                            ui.selectable_value(&mut self.audit_result, "success".to_string(), "Success");
                            ui.selectable_value(&mut self.audit_result, "failure".to_string(), "Failure");
                        });
                    ui.label("Target");
                    ui.add(egui::TextEdit::singleline(&mut self.audit_target_type).desired_width(75.0).hint_text("asset"));
                    ui.add(egui::TextEdit::singleline(&mut self.audit_target_id).desired_width(150.0).hint_text("id"));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("From");
                    ui.add(egui::TextEdit::singleline(&mut self.audit_from).desired_width(190.0).hint_text("RFC3339 timestamp"));
                    ui.label("To");
                    ui.add(egui::TextEdit::singleline(&mut self.audit_to).desired_width(190.0).hint_text("RFC3339 timestamp"));
                    if ui.button("Refresh Activity").clicked() {
                        self.refresh_activity();
                    }
                    if is_admin && ui.button("Export JSON + CSV").clicked() {
                        self.export_activity();
                    }
                    if ui.button("Clear Filters").clicked() {
                        self.audit_username.clear();
                        self.audit_action.clear();
                        self.audit_result.clear();
                        self.audit_target_type.clear();
                        self.audit_target_id.clear();
                        self.audit_from.clear();
                        self.audit_to.clear();
                        self.refresh_activity();
                    }
                });
                ui.separator();
                ui.label(format!("{} events", self.audit_events.len()));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for event in &self.audit_events {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(&event.action);
                                ui.label(format!("{} · HTTP {}", event.result, event.status_code));
                                ui.label(&event.occurred_at);
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!(
                                    "User: {} ({}) @ {}",
                                    event.actor_username.as_deref().unwrap_or("anonymous"),
                                    event.actor_role.as_deref().unwrap_or("unknown"),
                                    event.workstation.as_deref().unwrap_or("unknown")
                                ));
                                if let Some(kind) = event.target_type.as_deref() {
                                    ui.label(format!(
                                        "Target: {} {}",
                                        kind,
                                        event.target_id.as_deref().unwrap_or("")
                                    ));
                                }
                            });
                            ui.small(format!("{} {}", event.method, event.path));
                            if let Some(detail) = event.detail.as_deref() {
                                ui.small(format!("Detail: {detail}"));
                            }
                        });
                    }
                });
            });
        self.show_activity = open;
    }

    fn user_management_window(&mut self, ctx: &egui::Context) {
        if !self.show_users {
            return;
        }
        let mut open = self.show_users;
        egui::Window::new("DragonForge Users")
            .open(&mut open)
            .resizable(true)
            .default_width(700.0)
            .show(ctx, |ui| {
                ui.heading("Create User");
                ui.horizontal(|ui| {
                    ui.label("Username");
                    ui.text_edit_singleline(&mut self.new_user_name);
                    egui::ComboBox::from_id_salt("new_user_role")
                        .selected_text(self.new_user_role.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.new_user_role, UserRole::Administrator, "Administrator");
                            ui.selectable_value(&mut self.new_user_role, UserRole::Developer, "Developer");
                            ui.selectable_value(&mut self.new_user_role, UserRole::ReadOnly, "Read-only");
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("API Token");
                    ui.add(egui::TextEdit::singleline(&mut self.new_user_token).password(true).desired_width(300.0));
                    if ui.add_enabled(self.busy_count == 0, egui::Button::new("Create User")).clicked() {
                        self.create_vault_user();
                    }
                });
                ui.small("Tokens must be at least 16 characters. DragonForge stores only SHA-256 token hashes on the server.");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.heading("Existing Users");
                    if ui.button("Refresh").clicked() { self.refresh_users(); }
                });
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    for user in self.users.clone() {
                        let selected = self.selected_user_id.as_deref() == Some(user.id.as_str());
                        if ui.selectable_label(
                            selected,
                            format!("{} · {} · {}", user.username, user.role, if user.enabled { "enabled" } else { "disabled" })
                        ).clicked() {
                            self.selected_user_id = Some(user.id.clone());
                            self.selected_user_enabled = user.enabled;
                            self.selected_user_role = match user.role.as_str() {
                                "administrator" => UserRole::Administrator,
                                "read_only" => UserRole::ReadOnly,
                                _ => UserRole::Developer,
                            };
                            self.selected_user_token.clear();
                        }
                    }
                });
                if let Some(id) = self.selected_user_id.clone() {
                    if let Some(user) = self.users.iter().find(|user| user.id == id).cloned() {
                        ui.separator();
                        ui.heading(format!("Edit {}", user.username));
                        ui.checkbox(&mut self.selected_user_enabled, "Enabled");
                        egui::ComboBox::from_id_salt("selected_user_role")
                            .selected_text(self.selected_user_role.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.selected_user_role, UserRole::Administrator, "Administrator");
                                ui.selectable_value(&mut self.selected_user_role, UserRole::Developer, "Developer");
                                ui.selectable_value(&mut self.selected_user_role, UserRole::ReadOnly, "Read-only");
                            });
                        ui.horizontal(|ui| {
                            ui.label("New token (optional)");
                            ui.add(egui::TextEdit::singleline(&mut self.selected_user_token).password(true).desired_width(300.0));
                        });
                        ui.horizontal(|ui| {
                            if ui.add_enabled(self.busy_count == 0, egui::Button::new("Save User")).clicked() {
                                self.save_selected_user();
                            }
                            if ui.add_enabled(self.busy_count == 0, egui::Button::new("Delete User")).clicked() {
                                self.delete_selected_user();
                            }
                        });
                        if let Some(last) = user.last_used_at {
                            ui.small(format!("Last API use: {last}"));
                        }
                    }
                }
            });
        self.show_users = open;
    }

    fn project_sync_window(&mut self, ctx: &egui::Context) {
        if !self.show_project_sync {
            return;
        }
        let mut open = self.show_project_sync;
        let report = self.project_sync_report.clone();
        egui::Window::new("Project Sync Status")
            .open(&mut open)
            .resizable(true)
            .default_width(720.0)
            .show(ctx, |ui| {
                let Some(report) = report else {
                    ui.label("No project sync report loaded.");
                    return;
                };
                ui.heading(format!("{} ({})", report.project_name, report.engine));
                ui.label(format!("Export path: {}", report.export_path));
                ui.label(format!(
                    "Assets: {} · In sync: {} · Issues: {}",
                    report.items.len(),
                    report.in_sync_count(),
                    report.issue_count()
                ));
                for warning in &report.marker_warnings {
                    ui.label(format!("⚠ {warning}"));
                }
                ui.separator();
                egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
                    for item in &report.items {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(&item.asset_name);
                                ui.label(format!(
                                    "pinned v{} · vault v{}",
                                    item.pinned_version, item.current_version
                                ));
                                ui.label(item.status_label());
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.busy_count == 0, egui::Button::new("Check Again"))
                        .clicked()
                    {
                        self.check_selected_project_sync();
                    }
                    if ui
                        .add_enabled(self.busy_count == 0, egui::Button::new("Repair Pinned Files"))
                        .clicked()
                    {
                        self.repair_selected_project_sync();
                    }
                    if ui
                        .add_enabled(self.busy_count == 0, egui::Button::new("Update to Latest Revisions"))
                        .clicked()
                    {
                        self.update_selected_project_to_latest();
                    }
                });
            });
        self.show_project_sync = open;
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
                ui.horizontal(|ui| {
                    ui.label("Engine:");
                    egui::ComboBox::from_id_salt("project_engine")
                        .selected_text(&self.project_form.engine)
                        .show_ui(ui, |ui| {
                            if self.engine_presets.is_empty() {
                                ui.selectable_value(&mut self.project_form.engine, "Generic".to_string(), "Generic");
                            } else {
                                for preset in &self.engine_presets {
                                    ui.selectable_value(
                                        &mut self.project_form.engine,
                                        preset.id.clone(),
                                        &preset.display_name,
                                    );
                                }
                            }
                        });
                });

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

                if let Some(preset) = self.engine_presets.iter()
                    .find(|preset| preset.id.eq_ignore_ascii_case(&self.project_form.engine))
                {
                    ui.label(format!(
                        "Export location: <Project>/{}",
                        preset.export_subdir
                    ));
                    ui.small(&preset.import_notes);
                } else {
                    ui.label("Export location: <Project>/DragonForgeAssets");
                }

                ui.separator();
                if ui
                    .add_enabled(
                        self.busy_count == 0
                            && self.can_write()
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
        self.apply_phase16_style(ctx);
        self.handle_events(ctx);
        self.process_dropped_files(ctx);

        egui::SidePanel::left("phase161_sidebar")
            .default_width(self.settings.sidebar_width.clamp(190.0, 300.0))
            .resizable(false)
            .frame(egui::Frame::side_top_panel(&ctx.style()).fill(Self::bg()))
            .show(ctx, |ui| self.sidebar(ui));

        if self.current_view == AppView::Library {
            egui::SidePanel::left("phase161_asset_browser")
                .default_width(380.0)
                .min_width(320.0)
                .max_width(470.0)
                .resizable(true)
                .frame(egui::Frame::side_top_panel(&ctx.style()).fill(Self::panel_bg()))
                .show(ctx, |ui| {
                    ui.add_space(12.0);
                    self.filter_bar(ui);
                    ui.add_space(10.0);

                    if self.assets.is_empty() {
                        ui.add_space(50.0);
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                egui::RichText::new(if self.deleted_only {
                                    "Recycle bin is empty."
                                } else {
                                    "No matching assets."
                                })
                                .color(Self::muted_text()),
                            );
                        });
                    } else {
                        self.asset_grid(ui);
                    }
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(&ctx.style()).fill(Self::bg()))
                .show(ctx, |ui| {
                    ui.add_space(16.0);
                    self.details_panel(ui);
                });
        } else {
            egui::TopBottomPanel::top("phase161_header")
                .frame(egui::Frame::side_top_panel(&ctx.style()).fill(Self::panel_bg()))
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    self.top_bar(ui);
                    ui.add_space(10.0);
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(&ctx.style()).fill(Self::bg()))
                .show(ctx, |ui| {
                    ui.add_space(14.0);
                    match self.current_view {
                        AppView::Library => {}
                        AppView::Projects => self.projects_page(ui),
                        AppView::Activity => self.activity_page(ui),
                        AppView::Backups => self.backups_page(ui),
                        AppView::AiSearch => self.ai_page(ui),
                        AppView::Users => self.users_page(ui),
                        AppView::Settings => self.settings_page(ui, ctx),
                    }
                });
        }

        egui::TopBottomPanel::bottom("phase161_status")
            .frame(egui::Frame::side_top_panel(&ctx.style()).fill(Self::panel_bg()))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.small(egui::RichText::new(&self.status).color(Self::muted_text()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(
                            egui::RichText::new("DragonForge · Phase 16.1")
                                .color(Self::muted_text()),
                        );
                    });
                });
            });

        self.upload_window(ctx);
        self.edit_window(ctx);
        self.project_window(ctx);
        self.versions_window(ctx);
        self.package_import_window(ctx);
        self.package_contents_window(ctx);
        self.remove_project_confirm_window(ctx);
        self.project_sync_window(ctx);
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

fn default_ui_scale() -> f32 { 1.0 }
fn default_card_width() -> f32 { 220.0 }
fn default_sidebar_width() -> f32 { 205.0 }
fn default_inspector_width() -> f32 { 360.0 }
fn default_last_view() -> String { "library".to_string() }
fn default_true() -> bool { true }

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
        phase = 16,
        log_dir = %log_dir.display(),
        "DragonForge Phase 16.1 client starting"
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DragonForge Asset Vault")
            .with_inner_size([1500.0, 920.0])
            .with_min_inner_size([1080.0, 680.0])
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
