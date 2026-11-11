use std::path::Path as StdPath;
use std::sync::Arc;

use axum::Json;
use axum::extract::{ Multipart, Path, State };
use axum::http::StatusCode;
use bytes::Bytes;
use serde_json::{ Value, json };
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::AppError;
use crate::{ AppState, DEFAULT_MEMORY_LIMIT, Result};

pub async fn upload_tenant_service(
    State(state): State<Arc<AppState>>,
    path_param: Option<Path<Uuid>>,
    mut multipart: Multipart
) -> Result<(StatusCode, Json<Value>)> {
    // 1. Lấy thông tin tenant_id
    let (tenant_id, is_new) = match path_param {
        Some(Path(id)) => (id.to_string(), false),
        None => (Uuid::new_v4().to_string(), true),
    };

    // 2. Đọc dữ liệu và validate trước khi thao tác
    let mut wasm: Option<Bytes> = None;
    while let Some(field) = multipart
            .next_field().await
            .map_err(|_| AppError::BadRequest("Multipart không hợp lệ"))?
        {
            if field.name() != Some("file") {
                continue;
            }
            if wasm.is_some() {
                return Err(AppError::BadRequest("Chỉ được gửi một file wasm"));
            }
            let bytes = field.bytes().await.map_err(|_| AppError::BadRequest("Lỗi đọc dữ liệu file"))?;
            wasm = Some(bytes);
        }
    let wasm = wasm.ok_or(AppError::BadRequest("Thiếu field 'file'"))?;
    if !wasm.starts_with(b"\0asm") {
        return Err(AppError::BadRequest("File không đúng định dạng WebAssembly"));
    }
    //wasmtime::Module::validate(&Engine::default(), &wasm)
    //               .map_err(|_| AppError::BadRequest("File không đúng định dạng WebAssembly"));

    // 3. Chuẩn bị thư mục
    tokio::fs::create_dir_all(&state.root.registry).await?;
    tokio::fs::create_dir_all(&state.root.tenants).await?;

    // 4. Ghi wasm atomic, tên file = tenant_id
    // TODO thêm backup
    let wasm_path = state.root.registry.join(format!("{tenant_id}.wasm"));
    write_atomic(&wasm_path, &wasm).await?;

    // 5. Cập nhật dữ liệu theo tenant_id
    let db_path = state.root.tenants.join(format!("{tenant_id}.db"));
    state.db.upsert_tenant(
        &tenant_id,
        &wasm_path.to_string_lossy(),
        &db_path.to_string_lossy(),
        DEFAULT_MEMORY_LIMIT
    ).await?;

    // Clean cache
    state.runtime.invalidate(&tenant_id).await;

    let status = if is_new { StatusCode::CREATED } else { StatusCode::OK };

    //LOG

    Ok((
        status,
        Json(
            json!({
            "status": "success",
            "tenant_id": tenant_id,
            "message": format!("Tenant [{tenant_id}] xử lý Wasm service thành công!")
        })
        ),
    ))
}

async fn write_atomic(target: &StdPath, data: &[u8]) -> std::io::Result<()> {
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    // UUID để các request song song cho cùng tenant không đè file tạm của nhau
    let tmp = target.with_file_name(format!("{file_name}.{}.tmp", Uuid::new_v4()));

    let result = (async {
        let mut f = tokio::fs::File::create(&tmp).await?;
        f.write_all(data).await?;
        f.sync_all().await?; // đảm bảo dữ liệu đã xuống đĩa trước khi rename
        drop(f);
        tokio::fs::rename(&tmp, target).await
    }).await;

    if result.is_err() {
        // Dọn file tạm; file thật vẫn là bản cũ
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    result
}
