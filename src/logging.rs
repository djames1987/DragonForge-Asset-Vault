use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

pub fn init_file_logging(
    log_dir: &Path,
    filename_prefix: &str,
    default_filter: &str,
) -> anyhow::Result<WorkerGuard> {
    std::fs::create_dir_all(log_dir)?;

    let file_appender = tracing_appender::rolling::daily(log_dir, filename_prefix);
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(default_filter)),
        )
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .try_init()
        .map_err(|err| anyhow::anyhow!("failed to initialize logger: {err}"))?;

    Ok(guard)
}
