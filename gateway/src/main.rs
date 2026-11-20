mod db;
mod error;
mod logging;
mod router;
mod runtime_manager;
mod uploader;

use std::time::Duration;
use std::{env, path::PathBuf};

use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use wasmtime::component::{Resource, bindgen};
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

use crate::error::AppError;
use crate::db::{DatabaseManager, TenantMetadata};
use crate::logging::init_logging;
use crate::runtime_manager::TenantRuntime;

/// Thư mục mặc định lưu trữ file .wasm
const REGISTRY_NAME: &str = "registry";
/// Thư mục mặc định lưu trữ file data của các tenants
const TENANTS_NAME: &str = "tenants";
/// File data mặc định
const METADATA_NAME: &str = "metadata.db";
/// Giới hạn dung lượng mặc định
const DEFAULT_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// Chu kỳ tăng epoch của Engine.
const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Thời gian tối đa một request được chạy trong Wasm.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Giới hạn kích thước file wasm khi upload
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
/// Thời gian đợi giải phóng comile khỏi ram
const IDLE_TIMEOUT: u64 = 5 * 60;
/// Số lương cặp giá trị key-value được lưu trong cache
const MAX_CAPACITY: u64 = 100; 

pub type Result<T, E = AppError> = std::result::Result<T, E>;

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
    limits: StoreLimits,
    meta: TenantMetadata
}

impl TenantState {
    pub fn new(meta: &TenantMetadata) -> TenantState {
        TenantState { 
            wasi_ctx: WasiCtx::builder().inherit_stdout().inherit_stderr().build(),
            http: WasiHttpCtx::new(), 
            table: ResourceTable::new(), 
            limits: StoreLimitsBuilder::new()
                    .memory_size(DEFAULT_MEMORY_LIMIT)
                    .build(),
            meta: meta.clone()
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

bindgen!({
    path : "../wit/faas.wit",
    world: "faas",
    with : {
         "faas:platform/sqlite.connection": DbConn,
    },
});

pub struct DbConn {
    conn: Connection
}

pub fn convert_value(value: SqlValue) -> faas::platform::sqlite::Value {
    use faas::platform::sqlite::Value;
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(v) => Value::Integer(v),
        SqlValue::Real(v) => Value::Real(v),
        SqlValue::Text(v) => Value::Text(v),
        SqlValue::Blob(v) => Value::Blob(v),
    }
}


use faas::platform::logging::{Host as LogHost, Level};
use faas::platform::sqlite::{Host as SqliteHost, HostConnection, Row};

impl LogHost for  TenantState {
    fn log(&mut self, level: Level, message: String) {
        match level {
            Level::Error => tracing::error!("[Guest]-[{0}]: {1}", self.meta.tenant_id, message),
            Level::Warn => tracing::warn!("[Guest]-[{0}]: {1}", self.meta.tenant_id, message),
            Level::Info => tracing::info!("[Guest]-[{0}]: {1}", self.meta.tenant_id, message),
            Level::Debug => tracing::debug!("[Guest]-[{0}]: {1}", self.meta.tenant_id, message),
        }
    }
    
}

impl SqliteHost for TenantState {
    fn open(&mut self,) -> Result<Resource<DbConn>, String> {
        let db_path = self.meta.data_dir.clone();
        let conn = Connection::open(db_path).map_err(|e| e.to_string());
        match conn {
            Ok(conn) => {
                self.table.push(DbConn {conn}).map_err(|e| e.to_string())
            },
            Err(e) => Err(e),
        }
    }
}

impl HostConnection for TenantState {
    fn execute(&mut self, self_: Resource<DbConn>, sql: String, params: Vec<String>) -> Result<u64, String> {
        let db = self.table.get(&self_).map_err(|e|e.to_string())?;
        let count = db
                    .conn
                    .execute(&sql, params_from_iter(params))
                    .map_err(|e| e.to_string())?;
        Ok(count as u64)
    }

    fn query(&mut self, self_: Resource<DbConn>, sql: String, params: Vec<String>) -> Result<Vec<Row>, String> {
        use faas::platform::sqlite::Value;
        let db = self.table.get(&self_).map_err(|e| e.to_string())?;
        let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let col_count = stmt.column_count();

        let rows = stmt
                .query_map(params_from_iter(params), |row| {
                    let mut vals = Vec::with_capacity(col_count);
                    for i in 0..col_count {
                        let val = match row.get::<_, SqlValue>(i)? {
                                SqlValue::Null => Value::Null,
                                SqlValue::Integer(v) => Value::Integer(v),
                                SqlValue::Real(v) => Value::Real(v),
                                SqlValue::Text(v) => Value::Text(v),
                                SqlValue::Blob(v) => Value::Blob(v),
                        };
                        vals.push(val);
                    }
                    Ok(vals)
                })
                .map_err(|e| e.to_string())?;
        
        rows.collect::<std::result::Result<Vec<Row>, rusqlite::Error>>()
            .map_err(|e| e.to_string())

    }

    fn drop(&mut self, rep: Resource<DbConn>) -> wasmtime::Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 0. Khởi tạo subscriber 
    init_logging();

    // 1. Đọc biến môi trường
    dotenvy::dotenv().ok();
    let storage_dir_str = env::var("STORAGE_DIR")?;
    tracing::info!("[System]: Đường dẫn thư mục gốc: {}", &storage_dir_str);
    let storage_root = PathBuf::from(storage_dir_str);

    // 2. Khởi tạo các thư mục con trong storage nếu chưa tồn tại
    let registry_path = storage_root.join(REGISTRY_NAME);
    let tenants_path = storage_root.join(TENANTS_NAME);
    let metadata_path = storage_root.join(METADATA_NAME);
    tokio::fs::create_dir_all(&registry_path).await?;
    tokio::fs::create_dir_all(&tenants_path).await?;
    tracing::info!("[System]: Đường dẫn lưu file .wasm: {}", &registry_path.display());
    tracing::info!("[System]: Đường dẫn lưu database: {}", &tenants_path.display());
    tracing::info!("[System]: Đường dẫn metadata: {}", &metadata_path.display());
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

    // 6. Khởi tạo - chạy server
    let addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("[System]: Gateway lắng nghe tại {addr}");
    tracing::info!("[System]: Health http://{addr}/health");

    axum::serve(listener, router::build_router(state.into())).await?;
    Ok(())
}