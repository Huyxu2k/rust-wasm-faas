use std::sync::Arc;

use axum::{Router};

use crate::AppState;



pub fn build_router(state: Arc<AppState>) -> Router {
     Router::new()
     .with_state(state)
}