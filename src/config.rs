use serde::Deserialize;
use std::{env, path::{Path, PathBuf}};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_max_upload")]
    pub max_upload_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_filename")]
    pub filename: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            storage: StorageConfig::default(),
            database: DatabaseConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            max_upload_bytes: default_max_upload(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { data_dir: default_data_dir() }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self { filename: default_db_filename() }
    }
}

impl AppConfig {
    pub async fn load() -> anyhow::Result<Self> {
        let path = env::var("DRAGONFORGE_CONFIG").unwrap_or_else(|_| "DragonForge.toml".to_string());
        if !Path::new(&path).exists() {
            return Ok(Self::default());
        }
        let raw = tokio::fs::read_to_string(path).await?;
        Ok(toml::from_str(&raw)?)
    }

    pub fn database_path(&self) -> PathBuf {
        self.storage.data_dir.join(&self.database.filename)
    }
}

fn default_bind() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 8080 }
fn default_max_upload() -> usize { 8 * 1024 * 1024 * 1024 }
fn default_data_dir() -> PathBuf { PathBuf::from("./data") }
fn default_db_filename() -> String { "dragonforge.db".to_string() }
