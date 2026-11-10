use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{any, post, put},
};

use crate::{AppState, MAX_UPLOAD_BYTES, uploader::upload_tenant_service};

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/tenants",
            post(upload_tenant_service).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route(
            "/tenants/{tenant_id}",
            put(upload_tenant_service).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        //.route("/fn/{tenant_id}", any(invoke))
        //.route("/fn/{tenant_id}/{*rest}", any(invoke))
        //.route("/metrics", get(metrics_handler))
        .with_state(state)
}
