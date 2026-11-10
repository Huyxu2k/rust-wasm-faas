mod db;
mod error;
mod router;
mod runtime_manager;
mod uploader;

use std::time::Duration;
use std::{env, path::PathBuf};

use wasmtime::{Linker, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

use crate::error::AppError;
use crate::db::{DatabaseManager, TenantMetadata};
use crate::runtime_manager::TenantRuntime;

/// Thư mục mặc định lưu trữ file .wasm
const REGISTRY_NAME: &str = "registry";
/// Thư mục mặc định lưu trữ file data của các tenants
const TENANTS_NAME: &str = "tenants";
/// File data mặc định
const METADATA_NAME: &str = "metadata.db";
/// Giới hạn dung lượng mặc định
const DEFAULT_MEMORY_LIMIT: i64 = 64 * 1024 * 1024;
/// Chu kỳ tăng epoch của Engine.
const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Thời gian tối đa một request được chạy trong Wasm.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Giới hạn kích thước file wasm khi upload
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

pub type Result<T> = std::result::Result<T, AppError>;

/// Trạng thái chung
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseManager,
    pub root: AppPath,
    pub runtime: TenantRuntime
}
/// Lưu trữ đường dẫn chung khởi chạy server
#[derive(Clone)]
pub struct AppPath {
    pub registry: PathBuf,
    pub tenants: PathBuf,
    pub metadata: PathBuf
}

pub struct TenantState {
    wasi_ctx: WasiCtx,
    http: WasiHttpCtx,
    table: ResourceTable,
    limits: StoreLimits
}

impl TenantState {
    pub fn new(meta: &TenantMetadata) -> TenantState {
        TenantState { 
            wasi_ctx: WasiCtx::builder().inherit_stdout().inherit_stderr().build(),
            http: WasiHttpCtx::new(), 
            table: ResourceTable::new(), 
            limits: StoreLimitsBuilder::new()
                    .memory_size(meta.memory_limit_bytes as usize)
                    .build() 
        }
    }
}

impl WasiView for TenantState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table
        }
    }
}

impl WasiHttpView for TenantState {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView { 
            hooks: Default::default(), 
            table: &mut self.table, 
            ctx: &mut self.http
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Đọc biến môi trường
    dotenvy::dotenv().ok();
    let storage_dir_str = env::var("STORAGE_DIR")?;
    let storage_root = PathBuf::from(storage_dir_str);

    // 2. Khởi tạo các thư mục con trong storage nếu chưa tồn tại
    let registry_path = storage_root.join(REGISTRY_NAME);
    let tenants_path = storage_root.join(TENANTS_NAME);
    let metadata_path = storage_root.join(METADATA_NAME);
    tokio::fs::create_dir_all(&registry_path).await?;
    tokio::fs::create_dir_all(&tenants_path).await?;
    let app_path = AppPath { registry: registry_path, tenants: tenants_path, metadata: metadata_path.clone() };
    // 3. Metadata 
    let db_manager = DatabaseManager::new(&metadata_path)?;
    // 4. Khởi tạo runtime 
    let runtime_manager = TenantRuntime::new()?;
    // 5. AppState
    let state = AppState { 
        db: db_manager, 
        root: app_path, 
        runtime: runtime_manager
    };

    // TODO use LOG

    Ok(())
}