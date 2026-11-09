use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Error configuration : {0}")]
    Config(String),

    #[error("Error load Path: {0}")]
    PathError(String),

    #[error("Error wasmtime: {0}")]
    WasmtimeError(#[from] wasmtime::Error)
}
