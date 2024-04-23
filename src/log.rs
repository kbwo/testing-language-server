use tracing_appender::non_blocking::WorkerGuard;

pub struct Log;

impl Log {
    pub fn init() -> Result<WorkerGuard, anyhow::Error> {
        let home_dir = dirs::home_dir().unwrap();
        let log_path = home_dir.join(".config/testing_language_server/logs");
        let file_appender = tracing_appender::rolling::daily(log_path, "prefix.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        tracing_subscriber::fmt().with_writer(non_blocking).init();
        Ok(guard)
    }

    pub fn debug(msg: impl std::fmt::Debug) {
        tracing::debug!("{:?}", msg);
    }

    pub fn info(msg: impl std::fmt::Debug) {
        tracing::info!("{:?}", msg);
    }

    pub fn warn(msg: impl std::fmt::Debug) {
        tracing::warn!("{:?}", msg);
    }

    pub fn error(msg: impl std::fmt::Debug) {
        tracing::error!("{:?}", msg);
    }
}
