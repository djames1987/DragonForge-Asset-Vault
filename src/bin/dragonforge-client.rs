use eframe::egui;
use reqwest::blocking::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

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
    Download(Result<PathBuf, String>),
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
    tx: Sender<ClientEvent>,
    rx: Receiver<ClientEvent>,
}

impl DragonForgeClient {
    fn new() -> Self {
        let settings = load_settings().unwrap_or_default();
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
            tx,
            rx,
        };
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
            self.status = "Select a file to upload.".to_string();
            return;
        }
        let name = path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("New Asset")
            .to_string();
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

        thread::spawn(move || {
            let result = upload_asset(&base, &form_data);
            let _ = tx.send(ClientEvent::Upload(result));
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
        self.busy_count += 1;
        self.status = format!("Downloading {}...", asset.name);

        thread::spawn(move || {
            let result = download_asset(&base, &asset, &folder);
            let _ = tx.send(ClientEvent::Download(result));
        });
    }

    fn selected_asset(&self) -> Option<&Asset> {
        let id = self.selected_id.as_deref()?;
        self.assets.iter().find(|a| a.id == id)
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            self.busy_count = self.busy_count.saturating_sub(1);
            match event {
                ClientEvent::Health(Ok(health)) => {
                    self.status = format!(
                        "Connected to {} v{}",
                        health.service, health.version
                    );
                    self.health = Some(health);
                }
                ClientEvent::Health(Err(err)) => {
                    self.health = None;
                    self.status = format!("Connection failed: {err}");
                }
                ClientEvent::Assets(Ok(assets)) => {
                    self.assets = assets;
                    if self
                        .selected_id
                        .as_ref()
                        .is_some_and(|id| !self.assets.iter().any(|a| &a.id == id))
                    {
                        self.selected_id = None;
                    }
                    self.status = format!("{} assets loaded", self.assets.len());
                }
                ClientEvent::Assets(Err(err)) => {
                    self.status = format!("Could not load assets: {err}");
                }
                ClientEvent::Upload(Ok(response)) => {
                    self.show_upload = false;
                    self.selected_id = Some(response.asset.id.clone());
                    self.status = if response.duplicate {
                        format!("Already in vault: {}", response.asset.name)
                    } else {
                        format!("Added to vault: {}", response.asset.name)
                    };
                    self.refresh_assets();
                }
                ClientEvent::Upload(Err(err)) => {
                    self.status = format!("Upload failed: {err}");
                }
                ClientEvent::Download(Ok(path)) => {
                    self.status = format!("Downloaded to {}", path.display());
                }
                ClientEvent::Download(Err(err)) => {
                    self.status = format!("Download failed: {err}");
                }
            }
            ctx.request_repaint();
        }
    }

    fn process_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
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
                self.refresh_assets();
            }
            if ui.button("Connect").clicked() {
                let _ = save_settings(&self.settings);
                self.check_server();
                self.refresh_assets();
            }
            if ui.button("Refresh").clicked() {
                self.refresh_assets();
            }
            if self.busy_count > 0 {
                ui.spinner();
            }
        });

        ui.horizontal(|ui| {
            let connection = match &self.health {
                Some(h) if h.ok => format!(
                    "Connected · server phase {} · v{}",
                    h.phase, h.version
                ),
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
                || (enter && (search_response.has_focus() || category_response.has_focus()))
            {
                self.refresh_assets();
            }
            if ui.button("Clear").clicked() {
                self.search.clear();
                self.category_filter.clear();
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
                        let ext = asset
                            .extension
                            .as_deref()
                            .unwrap_or("file")
                            .to_ascii_uppercase();
                        let category = asset.category.as_deref().unwrap_or("Uncategorized");
                        let size = human_size(asset.byte_size);
                        let selected = self.selected_id.as_deref() == Some(asset.id.as_str());
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
                            .add_sized([235.0, 112.0], egui::Button::new(label).wrap())
                            .clicked()
                        {
                            chosen = Some(asset.id.clone());
                        }
                        if (index + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
        if let Some(id) = chosen {
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
                        .add_enabled(self.busy_count == 0, egui::Button::new("Upload to Vault"))
                        .clicked()
                    {
                        self.submit_upload();
                    }
                    if ui.button("Cancel").clicked() {
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
            .default_width(320.0)
            .resizable(true)
            .show(ctx, |ui| self.details_panel(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.assets.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("No matching assets. Add your first asset or change the search.");
                });
            } else {
                self.asset_grid(ui);
            }
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} assets shown", self.assets.len()));
                ui.separator();
                ui.label("DragonForge Client Phase 2");
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
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or("asset");
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

fn settings_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata)
            .join("DragonForge")
            .join("AssetVault")
            .join("client.json");
    }
    PathBuf::from("DragonForgeClient.json")
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DragonForge Asset Vault")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "DragonForge Asset Vault",
        options,
        Box::new(|_cc| Ok(Box::new(DragonForgeClient::new()))),
    )
}
