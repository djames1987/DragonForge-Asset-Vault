mod backup;
mod config;
mod db;
mod engine;
mod error;
mod logging;
mod licensing;
mod models;
mod package;
mod routes;
mod semantic;
mod storage;
mod thumbnail;
mod tiering;

use anyhow::Context;
use config::AppConfig;
use routes::AppState;
use std::net::SocketAddr;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load()
        .await
        .context("failed to load DragonForge configuration")?;

    tokio::fs::create_dir_all(&config.storage.data_dir)
        .await
        .context("failed to create vault data directory")?;

    let log_dir = config.storage.data_dir.join("logs");
    let _log_guard = logging::init_file_logging(
        &log_dir,
        "server.log",
        "dragonforge_server=info,tower_http=info",
    )?;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        phase = 14,
        log_dir = %log_dir.display(),
        "DragonForge server starting"
    );

    let pool = db::connect(&config.database_path())
        .await
        .context("failed to initialize asset catalog database")?;
    let storage = storage::Storage::new(
        config.storage.data_dir.clone(),
        config.storage.archive_dir.clone(),
    )
        .await
        .context("failed to initialize asset storage")?;

    info!(
        storage = %storage.root().display(),
        database = %config.database_path().display(),
        "vault storage initialized"
    );

    let app = routes::router(
        AppState {
            db: pool,
            storage,
            semantic: config.semantic.clone(),
            backup: config.backup.clone(),
            database_filename: config.database.filename.clone(),
        },
        config.server.max_upload_bytes,
    );

    let address: SocketAddr = format!("{}:{}", config.server.bind, config.server.port)
        .parse()
        .context("invalid server bind address")?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind DragonForge server to {address}"))?;

    info!(%address, "DragonForge Asset Vault Phase 14 is online");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    info!("DragonForge server stopped cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown requested");
}
