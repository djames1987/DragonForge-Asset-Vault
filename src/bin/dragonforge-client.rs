#[path = "../logging.rs"]
mod logging;

use eframe::egui;
use reqwest::blocking::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use tracing::{info, warn};

#[derive(Debug, Clone, Deserialize)]
struct HealthResponse {
    ok: bool,
    service: String,
    phase: u8,
    version: String,
}

#[derive(Debug, Clone, Deserialize)]
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
    tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UploadResponse {
    duplicate: bool,
    asset: Asset,
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

enum ClientEvent {
    Health(Result<HealthResponse, String>),
    Assets(Result<Vec<Asset>, String>),
    Upload(Result<UploadResponse, String>),
    Download {
        asset_id: String,
        result: Result<PathBuf, String>,
    },
    Thumbnail {
        asset_id: String,
        result: Result<Vec<u8>, String>,
    },
}

struct DragonForgeClient {
    settings: ClientSettings,
    assets: Vec<Asset>,
    selected_id: Option<String>,
    search: String,
    category_filter: String,
    status: String,
    health: Option<HealthResponse>,
    upload: UploadForm,
    show_upload: bool,
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
            selected_id: None,
            search: String::new(),
            category_filter: String::new(),
            status: "Ready".to_string(),
            health: None,
            upload: UploadForm::default(),
            show_upload: false,
            busy_count: 0,
            thumbnails: HashMap::new(),
            thumbnail_pending: HashSet::new(),
            tx,
            rx,
        };

        info!("DragonForge desktop client initialized");
        app.check_server();
        app.refresh_assets();
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

    fn refresh_assets(&mut self) {
        let tx = self.tx.clone();
        let base = self.base_url();
        let search = self.search.trim().to_string();
        let category = self.category_filter.trim().to_string();
        self.busy_count += 1;
        self.status = "Loading assets...".to_string();

        info!(
            server_url = %base,
            search = %search,
            category = %category,
            "asset refresh started"
        );

        thread::spawn(move || {
            let result = (|| -> Result<Vec<Asset>, String> {
                let client = Self::api_client()?;
                let mut request = client.get(format!("{base}/api/assets"));
                if !search.is_empty() {
                    request = request.query(&[("q", search.as_str())]);
                }
                if !category.is_empty() {
                    request = request.query(&[("category", category.as_str())]);
                }
                request
                    .send()
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .json()
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(ClientEvent::Assets(result));
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

        info!(
            path = %path.display(),
            suggested_name = %name,
            "asset selected for upload"
        );

        self.upload = UploadForm {
            file_path: Some(path),
            name,
            ..UploadForm::default()
        };
        self.show_upload = true;
    }

    fn submit_upload(&mut self) {
        let Some(file_path) = self.upload.file_path.clone() else {
            warn!("upload attempted without selected file");
            self.status = "No upload file selected.".to_string();
            return;
        };

        let tx = self.tx.clone();
        let base = self.base_url();
        let form_data = self.upload.clone();
        self.busy_count += 1;
        self.status = format!("Uploading {}...", file_path.display());

        info!(
            path = %file_path.display(),
            name = %form_data.name,
            category = %form_data.category,
            tags = %form_data.tags,
            creator = %form_data.creator,
            license = %form_data.license,
            attribution_required = form_data.attribution_required,
            "asset upload started"
        );

        thread::spawn(move || {
            let result = upload_asset(&base, &form_data);
            let _ = tx.send(ClientEvent::Upload(result));
        });
    }

    fn download_selected(&mut self) {
        let Some(asset) = self.selected_asset().cloned() else {
            warn!("download requested with no selected asset");
            self.status = "Select an asset first.".to_string();
            return;
        };

        let Some(folder) = rfd::FileDialog::new()
            .set_title("Choose download folder")
            .pick_folder()
        else {
            info!(asset_id = %asset.id, "download folder selection cancelled");
            return;
        };

        let tx = self.tx.clone();
        let base = self.base_url();
        let asset_id = asset.id.clone();
        self.busy_count += 1;
        self.status = format!("Downloading {}...", asset.name);

        info!(
            asset_id = %asset.id,
            name = %asset.name,
            destination = %folder.display(),
            "asset download started"
        );

        thread::spawn(move || {
            let result = download_asset(&base, &asset, &folder);
            let _ = tx.send(ClientEvent::Download { asset_id, result });
        });
    }

    fn selected_asset(&self) -> Option<&Asset> {
        let id = self.selected_id.as_deref()?;
        self.assets.iter().find(|a| a.id == id)
    }

    fn request_missing_thumbnails(&mut self) {
        let candidates: Vec<String> = self
            .assets
            .iter()
            .filter(|asset| is_previewable_extension(asset.extension.as_deref()))
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

            info!(asset_id = %asset_id, "thumbnail fetch started");

            thread::spawn(move || {
                let result = (|| -> Result<Vec<u8>, String> {
                    let client = Self::api_client()?;
                    let bytes = client
                        .get(format!("{base}/api/assets/{request_id}/thumbnail"))
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
                ClientEvent::Health(Ok(health)) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    info!(
                        service = %health.service,
                        phase = health.phase,
                        version = %health.version,
                        "server connection successful"
                    );
                    self.status =
                        format!("Connected to {} v{}", health.service, health.version);
                    self.health = Some(health);
                }
                ClientEvent::Health(Err(err)) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    warn!(error = %err, "server connection failed");
                    self.health = None;
                    self.status = format!("Connection failed: {err}");
                }
                ClientEvent::Assets(Ok(assets)) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    info!(count = assets.len(), "asset refresh completed");
                    self.assets = assets;

                    if self
                        .selected_id
                        .as_ref()
                        .is_some_and(|id| !self.assets.iter().any(|a| &a.id == id))
                    {
                        info!("selected asset cleared because it is not in current results");
                        self.selected_id = None;
                    }

                    self.status = format!("{} assets loaded", self.assets.len());
                    self.request_missing_thumbnails();
                }
                ClientEvent::Assets(Err(err)) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    warn!(error = %err, "asset refresh failed");
                    self.status = format!("Could not load assets: {err}");
                }
                ClientEvent::Upload(Ok(response)) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    self.show_upload = false;
                    self.selected_id = Some(response.asset.id.clone());

                    if response.duplicate {
                        info!(
                            asset_id = %response.asset.id,
                            name = %response.asset.name,
                            sha256 = %response.asset.sha256,
                            "duplicate upload blocked by vault"
                        );
                        self.status =
                            format!("Already in vault: {}", response.asset.name);
                    } else {
                        info!(
                            asset_id = %response.asset.id,
                            name = %response.asset.name,
                            sha256 = %response.asset.sha256,
                            "asset upload completed"
                        );
                        self.status =
                            format!("Added to vault: {}", response.asset.name);
                    }
                    self.refresh_assets();
                }
                ClientEvent::Upload(Err(err)) => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    warn!(error = %err, "asset upload failed");
                    self.status = format!("Upload failed: {err}");
                }
                ClientEvent::Download { asset_id, result } => {
                    self.busy_count = self.busy_count.saturating_sub(1);
                    match result {
                        Ok(path) => {
                            info!(
                                asset_id = %asset_id,
                                path = %path.display(),
                                "asset download completed"
                            );
                            self.status = format!("Downloaded to {}", path.display());
                        }
                        Err(err) => {
                            warn!(
                                asset_id = %asset_id,
                                error = %err,
                                "asset download failed"
                            );
                            self.status = format!("Download failed: {err}");
                        }
                    }
                }
                ClientEvent::Thumbnail { asset_id, result } => {
                    self.thumbnail_pending.remove(&asset_id);
                    match result {
                        Ok(bytes) => match self.install_thumbnail(ctx, &asset_id, &bytes) {
                            Ok(()) => {
                                info!(
                                    asset_id = %asset_id,
                                    byte_size = bytes.len(),
                                    "thumbnail loaded into client"
                                );
                            }
                            Err(err) => {
                                warn!(
                                    asset_id = %asset_id,
                                    error = %err,
                                    "thumbnail decode failed"
                                );
                            }
                        },
                        Err(err) => {
                            warn!(
                                asset_id = %asset_id,
                                error = %err,
                                "thumbnail fetch failed"
                            );
                        }
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
                match save_settings(&self.settings) {
                    Ok(()) => info!(server_url = %self.settings.server_url, "client settings saved"),
                    Err(err) => warn!(error = %err, "client settings save failed"),
                }
                self.check_server();
                self.refresh_assets();
            }
            if ui.button("Connect").clicked() {
                info!(server_url = %self.settings.server_url, "connect button clicked");
                match save_settings(&self.settings) {
                    Ok(()) => info!(server_url = %self.settings.server_url, "client settings saved"),
                    Err(err) => warn!(error = %err, "client settings save failed"),
                }
                self.check_server();
                self.refresh_assets();
            }
            if ui.button("Refresh").clicked() {
                info!("refresh button clicked");
                self.refresh_assets();
            }
            if self.busy_count > 0 {
                ui.spinner();
            }
        });

        ui.horizontal(|ui| {
            let connection = match &self.health {
                Some(h) if h.ok => {
                    format!("Connected · server phase {} · v{}", h.phase, h.version)
                }
                _ => "Not connected".to_string(),
            };
            ui.label(connection);
            ui.separator();
            ui.label(&self.status);
        });
    }

    fn filter_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Search");
            let search_response = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("sword, medieval, creator, license..."),
            );
            ui.label("Category");
            let category_response = ui.add(
                egui::TextEdit::singleline(&mut self.category_filter)
                    .hint_text("weapon, prop, texture..."),
            );
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Search").clicked()
                || (enter
                    && (search_response.has_focus() || category_response.has_focus()))
            {
                info!(
                    search = %self.search,
                    category = %self.category_filter,
                    "search submitted"
                );
                self.refresh_assets();
            }
            if ui.button("Clear").clicked() {
                info!("search filters cleared");
                self.search.clear();
                self.category_filter.clear();
                self.refresh_assets();
            }
            ui.separator();
            if ui.button("Add Asset").clicked() {
                info!("add asset button clicked");
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Select an asset to add")
                    .pick_file()
                {
                    self.begin_upload(path);
                } else {
                    info!("asset file selection cancelled");
                }
            }
        });
        ui.label("Tip: drag any file onto this window to add it to the vault.");
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
                            let category =
                                asset.category.as_deref().unwrap_or("Uncategorized");
                            let size = human_size(asset.byte_size);
                            let selected =
                                self.selected_id.as_deref() == Some(asset.id.as_str());
                            let marker = if selected { "● " } else { "" };
                            let label = format!(
                                "{marker}{}\n{} · {}\n{}\n{}",
                                asset.name,
                                ext,
                                size,
                                category,
                                if asset.tags.is_empty() {
                                    "No tags".to_string()
                                } else {
                                    asset.tags.join(", ")
                                }
                            );

                            if ui
                                .add_sized(
                                    [210.0, 88.0],
                                    egui::Button::new(label).wrap(),
                                )
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
        ui.heading("Asset Details");
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
        ui.label(format!(
            "Type: {}",
            asset.extension.as_deref().unwrap_or("unknown")
        ));
        ui.label(format!(
            "Category: {}",
            asset.category.as_deref().unwrap_or("Uncategorized")
        ));
        ui.label(format!("Added: {}", asset.created_at));
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
        ui.label(format!(
            "Creator: {}",
            asset.creator.as_deref().unwrap_or("Unknown")
        ));
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
        if let Some(source) = &asset.source_url {
            ui.label(format!("Source: {source}"));
        }

        ui.separator();
        ui.label("SHA-256");
        ui.small(&asset.sha256);
        ui.add_space(8.0);

        if ui.button("Download Asset").clicked() {
            info!(asset_id = %asset.id, "download asset button clicked");
            self.download_selected();
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
                    if let Ok(metadata) = fs::metadata(path) {
                        ui.label(format!("Size: {}", human_size(metadata.len() as i64)));
                    }
                }

                ui.separator();
                form_row(ui, "Name", &mut self.upload.name);
                form_row(ui, "Category", &mut self.upload.category);
                form_row(ui, "Tags", &mut self.upload.tags);
                ui.small("Separate tags with commas, e.g. fantasy, medieval, sword.");
                form_row(ui, "Creator", &mut self.upload.creator);
                form_row(ui, "Source URL", &mut self.upload.source_url);
                form_row(ui, "License", &mut self.upload.license);
                ui.checkbox(
                    &mut self.upload.attribution_required,
                    "Attribution required",
                );
                ui.label("Description");
                ui.add(
                    egui::TextEdit::multiline(&mut self.upload.description)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.busy_count == 0,
                            egui::Button::new("Upload to Vault"),
                        )
                        .clicked()
                    {
                        info!("upload to vault button clicked");
                        self.submit_upload();
                    }
                    if ui.button("Cancel").clicked() {
                        info!("upload dialog cancelled");
                        self.show_upload = false;
                    }
                });
            });
        self.show_upload = open && self.show_upload;
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
            .default_width(340.0)
            .resizable(true)
            .show(ctx, |ui| self.details_panel(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.assets.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        "No matching assets. Add your first asset or change the search.",
                    );
                });
            } else {
                self.asset_grid(ui);
            }
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} assets shown", self.assets.len()));
                ui.separator();
                ui.label("DragonForge Client Phase 3");
                ui.separator();
                ui.label(format!("Logs: {}", client_log_dir().display()));
            });
        });

        self.upload_window(ctx);
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
        Some("jpg") | Some("jpeg") | Some("png") | Some("webp")
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
        "dragonforge_asset_vault=info,dragonforge_client=info,info",
    ) {
        Ok(guard) => Some(guard),
        Err(err) => {
            eprintln!("DragonForge client logger failed to initialize: {err}");
            None
        }
    };

    info!(
        version = env!("CARGO_PKG_VERSION"),
        phase = 3,
        log_dir = %log_dir.display(),
        "DragonForge client starting"
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DragonForge Asset Vault")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 620.0])
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
