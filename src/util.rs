use crate::error::LSError;
use serde::Serialize;
use std::io::stdout;
use std::io::Write;

pub fn send_stdout<T>(message: &T) -> Result<(), LSError>
where
    T: ?Sized + Serialize + std::fmt::Debug,
{
    tracing::info!("send stdout: {:#?}", message);
    let msg = serde_json::to_string(message)?;
    let mut stdout = stdout().lock();
    write!(stdout, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
    stdout.flush()?;
    Ok(())
}

pub fn format_uri(uri: &str) -> String {
    uri.replace("file://", "")
}
