use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("rule file error in {file}: {message}")]
    Rules { file: String, message: String },
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("blocked for safety: {path} ({reason})")]
    Unsafe { path: PathBuf, reason: String },
    #[error("administrator rights needed: {0}")]
    NeedsAdmin(String),
    #[error("not supported on this system: {0}")]
    NotSupported(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("registry error: {0}")]
    Registry(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
