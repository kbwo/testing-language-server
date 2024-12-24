use std::path::PathBuf;

use testing_language_server::util::clean_old_logs;
use tracing_appender::non_blocking::WorkerGuard;

pub struct Log;

impl Log {
    fn log_dir() -> PathBuf {
        let home_dir = dirs::home_dir().unwrap();

        home_dir.join(".config/testing_language_server/adapter/logs")
    }

    pub fn init() -> Result<WorkerGuard, anyhow::Error> {
        let log_dir_path = Self::log_dir();
        let prefix = "adapter.log";
        let file_appender = tracing_appender::rolling::daily(&log_dir_path, prefix);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        clean_old_logs(
            log_dir_path.to_str().unwrap(),
            30,
            &format!("{prefix}.*"),
            &format!("{prefix}."),
        )
        .unwrap();
        tracing_subscriber::fmt().with_writer(non_blocking).init();
        Ok(guard)
    }
}
