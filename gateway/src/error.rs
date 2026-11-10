use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    /// Lỗi nội bộ
    #[error("Error wasmtime: {0}")]
    WasmtimeError(#[from] wasmtime::Error),

    #[error("Error env: {0}")]
    EnvError(#[from] std::env::VarError),

    #[error("Error io: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Error sqlite: {0}")]
    SqliteError(#[from] rusqlite::Error),

    /// Error Service
    #[error("BadRequest: {0}")]
    BadRequest(&'static str),
    #[error("Internal: {0}")]
    Internal(anyhow::Error),
    #[error("NotFound: {0}")]
    NotFound(&'static str),
    #[error("Server đang quá tải")]
    Busy,
    #[error("Request vượt quá thời gian cho phép")]
    Timeout,
    #[error("Guest trap: {0}")]
    Trap(String),
    #[error("Guest error: {0}")]
    Guest(String),
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode as S;
        let (status, msg) = match &self {
            AppError::BadRequest(_) => (S::BAD_GATEWAY, self.to_string()),
            AppError::NotFound(_) => (S::NOT_FOUND, self.to_string()),
            AppError::Busy => (S::SERVICE_UNAVAILABLE, self.to_string()),
            AppError::Timeout => (S::GATEWAY_TIMEOUT, self.to_string()),
            AppError::Trap(_) | AppError::Guest(_) => {
                (S::BAD_GATEWAY, "Lỗi Function khi xử lý".into())
            }

            AppError::WasmtimeError(_)
            | AppError::EnvError(_)
            | AppError::IOError(_)
            | AppError::SqliteError(_)
            | AppError::Internal(_)
            | _ => (S::INTERNAL_SERVER_ERROR, "Internal error".into()),
        };

        // if status.is_server_error() {
        //     //LOG
        // }

        (
            status,
            axum::Json(serde_json::json!(
               {
                    "status": "error",
                    "message" : msg
               }
            )),
        )
            .into_response()
    }
}
