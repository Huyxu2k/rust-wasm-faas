use std::{collections::HashMap, result, sync::Arc, time::Instant};

use axum::{
    Router, body::Body, extract::{DefaultBodyLimit, Path, Request, State}, http::{Uri, header::HOST}, response::{IntoResponse, Response}, routing::{any, get, post, put},
};
use http_body_util::BodyExt;
use wasmtime::{Store, Trap};
use wasmtime_wasi_http::{WasiHttpView, p2::bindings::http::types::Scheme};

use crate::{
    AppState, EPOCH_TICK, MAX_UPLOAD_BYTES, REQUEST_TIMEOUT, Result, TenantState, error::AppError,
    uploader::upload_tenant_service,
};

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/tenants/upload",
            post(upload_tenant_service).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route(
            "/tenants/{tenant_id}/upload",
            put(upload_tenant_service).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route("/fn/{tenant_id}", any(invoke))
        .route("/fn/{tenant_id}/{*rest}", any(invoke))
        //.route("/metrics", get(metrics_handler))
        .route("/health", get(health_check))
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    "Success".into_response()
}

async fn invoke(State(state): State<Arc<AppState>>, Path(params): Path<HashMap<String, String>>, req: Request) -> Response {
    let tenant_id = params.get("tenant_id").cloned().unwrap_or_default();

    let result = run_guest(&state, &tenant_id, req).await;

    match result {
        Ok(resp) => resp,
        Err(e) => e.into_response(),
    }
} 

async fn run_guest(state: &AppState, tenant_id: &str, req: Request) -> Result<Response> {
    // 0. tenant_id không đúng định dạng UUID loại
    if uuid::Uuid::parse_str(tenant_id).is_err() {
        return Err(AppError::NotFound("Tenant không tồn tại"));
    }
    
    // 1. Tra metadata + lấy ProxyPre đã compile
    let tenant = state.runtime.get(&tenant_id, &state.db).await?;

    // 2. Store mới cho request
    let mut store = Store::new(&state.runtime.engine, TenantState::new(&tenant.meta));
    store.limiter(|s| &mut s.limits);
    store.set_epoch_deadline(1); // Mỗi tick (10ms) guest nhường luồng cho task khác
    store.epoch_deadline_async_yield_and_update(1);

    // 3. Chuyển request axum thành resource wasi:http
    let req = rewrite_uri(req, tenant_id)?;
    let req =
        req.map(|b| b.map_err(|e| wasmtime_wasi_http::Error::InternalError(Some(e.to_string()))));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let wasi_req = store
        .data_mut()
        .http()
        .new_incoming_request(Scheme::Http, req)?;
    let out = store.data_mut().http().new_response_outparam(tx)?;

    // 4. Instantiate
    let proxy = tenant.pre
        .instantiate_async(&mut store)
        .await
        .map_err(map_wasm_err)?;

    // 5. Chạy hander + xử lý giới hạn thời gian chạy
    let tenant = tenant_id.to_owned();
    let task = tokio::spawn(async move {
        let res = proxy
            .wasi_http_incoming_handler()
            .call_handle(&mut store, wasi_req, out);
        
        match tokio::time::timeout(REQUEST_TIMEOUT, res).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                tracing::warn!(tenant = %tenant, error = %format!("{e:#}"), "guest handler lỗi");
                Err(map_wasm_err(e))
            },
            Err(_) => {
                tracing::warn!(tenant = %tenant, "guest chạy quá {REQUEST_TIMEOUT:?} → huỷ");
                Err(AppError::Timeout)
            },
        }
    });

    // 6. Chờ guest gọi response-outparam.set
    match rx.await {
        Ok(Ok(resp)) => Ok(resp.map(Body::new)),
        Ok(Err(code)) => Err(AppError::Guest(format!("{code:?}"))),
        Err(_) => match task.await {
            Ok(Ok(())) => Err(AppError::Guest("Guest không trả response".into())),
            Ok(Err(e)) => Err(e),
            Err(join) => Err(AppError::Internal(join.into())),
        },
    }
}

fn rewrite_uri(mut req: Request, tenant_id: &str) -> Result<Request> {
    let prefix = format!("/fn/{tenant_id}");
    let pq = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let rest = pq.strip_prefix(prefix.as_str()).unwrap_or(pq);
    let rest = if rest.is_empty() || rest.starts_with("?") {
        format!("/{rest}")
    } else {
        rest.to_string()
    };

    let host = req
        .headers()
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost")
        .to_owned();

    *req.uri_mut() = Uri::builder()
        .scheme("http")
        .authority(host)
        .path_and_query(rest)
        .build()
        .map_err(|_| AppError::BadRequest("URI không hợp lệ"))?;

    Ok(req)
}

fn epoch_ticks() -> u64 {
    (REQUEST_TIMEOUT.as_millis() / EPOCH_TICK.as_millis()).max(1) as u64
}

fn map_wasm_err(e: wasmtime::Error) -> AppError {
    if e.downcast_ref::<wasmtime::PoolConcurrencyLimitError>()
        .is_some()
    {
        return AppError::Busy;
    }

    match e.downcast_ref::<Trap>() {
        Some(Trap::Interrupt) => AppError::Timeout,
        Some(_) => AppError::Trap(format!("{e:#}")),
        None => AppError::WasmtimeError(e),
    }
}
