use rusqlite::{params, Connection, Result};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct TenantMetadata {
    pub tenant_id: String,
    pub wasm_path: String,
    pub data_dir: String,
    pub memory_limit_bytes: i64
}

#[derive(Clone)]
pub struct DatabaseManager {
    conn: Arc<Mutex<Connection>>
}

impl DatabaseManager {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        // Tạo bảng tenants nếu chưa có
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tenants(
                    tenant_id TEXT PRIMARY KEY,
                    wasm_path TEXT NOT NULL,
                    data_dir TEXT NOT NULL,
                    memory_limit_bytes INTEGER NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )", 
            [],
        )?;

        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub async fn upsert_tenant(
        &self,
        tenant_id: &str,
        wasm_path: &str,
        data_dir: &str,
        memory_limit_bytes: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO tenants (tenant_id, wasm_path, data_dir, memory_limit_bytes)
                    VALUES (?1, ?2, ?3, ?4)
                    ON CONFLICT(tenant_id) DO UPDATE SET
                        wasm_path = excluded.wasm_path,
                        data_dir = excluded.data_dir,
                        memory_limit_bytes = excluded.memory_limit_bytes",
            params![tenant_id, wasm_path, data_dir, memory_limit_bytes],
        )?;
        Ok(())
    } 

    pub async fn get_tenant(&self, tenant_id: &str) -> Result<Option<TenantMetadata>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT tenant_id, wasm_path, data_dir, memory_limit_bytes FROM tenants WHERE tenant_id = ?1",
        )?;

        let mut rows = stmt.query(params![tenant_id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(TenantMetadata {
                tenant_id: row.get(0)?,
                wasm_path: row.get(1)?,
                data_dir: row.get(2)?,
                memory_limit_bytes: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }
}