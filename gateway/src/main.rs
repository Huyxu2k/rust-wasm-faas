mod db;
mod error;
mod runtime_manager;

use std::{env, path::PathBuf};

use crate::error::AppError;
use crate::db::DatabaseManager;

pub type Result<T> = std::result::Result<T, AppError>;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Đọc biến môi trường
    dotenvy::dotenv().ok();
    let storage_dir_str = env::var("STORAGE_DIR")?;
    let storage_root = PathBuf::from(storage_dir_str);

    // 2. Khởi tạo các thư mục con trong storage nếu chưa tồn tại
    let registry_path = storage_root.join("registry");
    let tenants_db_path = storage_root.join("tenants");
    let metadata_db_path = storage_root.join("metadata.db");
    tokio::fs::create_dir_all(&registry_path).await?;
    tokio::fs::create_dir_all(&tenants_db_path).await?;

    let db_manager = DatabaseManager::new(&metadata_db_path)?;
    // TODO use LOG
    println!(
        "[System Config]: Thư mục Storage gốc đang trỏ tới: {:?}",
        storage_root.canonicalize()?
    );
    println!("[System Config]: Database metadata: {:?}", metadata_db_path);

    Ok(())
}
