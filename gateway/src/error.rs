
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Error configuration : {0}")]
    ConfigError(String),

    #[error("Error load Path: {0}")]
    PathError(String),

    #[error("Error wasmtime: {0}")]
    WasmtimeError(#[from] wasmtime::Error),

    #[error("Error env: {0}")]
    EnvError(#[from] std::env::VarError),

    #[error("Error io: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Error sqlite: {0}")]
    SqliteError(#[from] rusqlite::Error),
}
