pub mod migrations;
pub mod repository;
pub mod settings_store;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// 打开数据库并执行 migration。启动路径失败必须直接返回错误，
/// 不允许应用在未知结构的数据库上继续初始化（数据文档第 5 节）。
pub fn open_and_migrate(db_path: &Path) -> rusqlite::Result<Arc<Mutex<Connection>>> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            rusqlite::Error::InvalidPath(std::path::PathBuf::from(format!(
                "无法创建数据目录 {}: {e}",
                parent.display()
            )))
        })?;
    }

    let mut conn = Connection::open(db_path)?;
    migrations::run_migrations(&mut conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}
