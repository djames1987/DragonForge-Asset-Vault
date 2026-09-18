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
    #[serde(default)]
    pub semantic: SemanticConfig,
    #[serde(default)]
    pub backup: BackupConfig,
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

#[derive(Debug, Clone, Deserialize)]
pub struct BackupConfig {
    #[serde(default = "default_backup_dir")]
    pub directory: PathBuf,
    #[serde(default)]
    pub replication_targets: Vec<PathBuf>,
    #[serde(default = "default_backup_keep")]
    pub keep: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticConfig {
    #[serde(default = "default_semantic_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_semantic_weight")]
    pub semantic_weight: f32,
    #[serde(default = "default_keyword_weight")]
    pub keyword_weight: f32,
    #[serde(default = "default_semantic_limit")]
    pub max_results: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            storage: StorageConfig::default(),
            database: DatabaseConfig::default(),
            semantic: SemanticConfig::default(),
            backup: BackupConfig::default(),
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


impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            enabled: default_semantic_enabled(),
            ollama_url: default_ollama_url(),
            model: default_embedding_model(),
            semantic_weight: default_semantic_weight(),
            keyword_weight: default_keyword_weight(),
            max_results: default_semantic_limit(),
        }
    }
}

fn default_semantic_enabled() -> bool { true }
fn default_ollama_url() -> String { "http://127.0.0.1:11434".to_string() }
fn default_embedding_model() -> String { "nomic-embed-text".to_string() }
fn default_semantic_weight() -> f32 { 0.70 }
fn default_keyword_weight() -> f32 { 0.30 }
fn default_semantic_limit() -> usize { 100 }


impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            directory: default_backup_dir(),
            replication_targets: Vec::new(),
            keep: default_backup_keep(),
        }
    }
}

fn default_backup_dir() -> PathBuf { PathBuf::from("./backups") }
fn default_backup_keep() -> usize { 10 }
