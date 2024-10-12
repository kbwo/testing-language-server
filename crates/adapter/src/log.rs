use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;

pub struct Log;

impl Log {
    fn log_dir() -> PathBuf {
        let home_dir = dirs::home_dir().unwrap();
        let log_path = home_dir.join(".config/testing_language_server/adapter/logs");
        log_path
    }

    pub fn init() -> Result<WorkerGuard, anyhow::Error> {
        let log_path = Self::log_dir();
        let file_appender = tracing_appender::rolling::daily(log_path, "prefix.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        tracing_subscriber::fmt().with_writer(non_blocking).init();
        Ok(guard)
    }
}
