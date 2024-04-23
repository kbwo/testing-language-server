use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LSError {
    #[error("IO error")]
    IO(#[from] io::Error),

    #[error("Serialization error")]
    Serialization(#[from] serde_json::Error),

    #[error("Adapter error")]
    Adapter(String),

    #[error("UTF8 error")]
    UTF8(#[from] std::str::Utf8Error),

    #[error("From UTF8 error")]
    FromUTF8(#[from] std::string::FromUtf8Error),

    #[error("Unknown error")]
    Any(#[from] anyhow::Error),
}
