use chrono::Local;
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};
use tracing_subscriber::{fmt::MakeWriter, EnvFilter};

pub struct LoggerGuard;

#[derive(Clone)]
struct DailyFileWriter {
    log_dir: PathBuf,
    filename_prefix: String,
}

impl DailyFileWriter {
    fn current_path(&self) -> PathBuf {
        let date = Local::now().format("%Y-%m-%d");
        self.log_dir
            .join(format!("{}.{}", self.filename_prefix, date))
    }

    fn open(&self) -> io::Result<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.current_path())
    }
}

impl<'a> MakeWriter<'a> for DailyFileWriter {
    type Writer = File;

    fn make_writer(&'a self) -> Self::Writer {
        self.open()
            .expect("DragonForge logger could not open the log file")
    }
}

pub fn init_file_logging(
    log_dir: &Path,
    filename_prefix: &str,
    default_filter: &str,
) -> anyhow::Result<LoggerGuard> {
    std::fs::create_dir_all(log_dir)?;

    let writer = DailyFileWriter {
        log_dir: log_dir.to_path_buf(),
        filename_prefix: filename_prefix.to_string(),
    };

    // Prove the log destination is writable before installing the subscriber.
    writer.open()?;

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

    Ok(LoggerGuard)
}
