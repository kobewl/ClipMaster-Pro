pub mod agent_run_store;
pub mod chat_store;
pub mod group_repository;
pub mod migrations;
pub mod repository;
pub mod session_store;
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
    run_maintenance(&conn);
    Ok(Arc::new(Mutex::new(conn)))
}

/// 启动时的轻量维护：`PRAGMA optimize=0x10002`（SQLite 对长连接在打开时的推荐
/// 用法），只重新分析统计信息过期的表，代价很低。失败不影响启动，只记日志。
///
/// 刻意不改 auto_vacuum：切换它需要整库 VACUUM，不适合放在启动路径上。
pub fn run_maintenance(conn: &Connection) {
    if let Err(err) = conn.execute_batch("PRAGMA optimize=0x10002;") {
        tracing::warn!("PRAGMA optimize 失败（不影响使用）：{err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_migrate_runs_maintenance_and_reaches_the_declared_version() {
        let path = std::env::temp_dir().join(format!("cm-maint-{}.db", uuid::Uuid::new_v4()));
        let result = open_and_migrate(&path).map(|conn| {
            conn.lock().unwrap().query_row(
                "SELECT MAX(version) FROM schema_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
        });
        for suffix in ["", "-wal", "-shm"] {
            let mut file = path.clone().into_os_string();
            file.push(suffix);
            let _ = std::fs::remove_file(file);
        }
        let version = result.expect("打开并迁移").expect("读版本号");
        assert_eq!(version, migrations::CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn run_maintenance_leaves_database_queryable() {
        let mut conn = Connection::open_in_memory().expect("内存库");
        migrations::run_migrations(&mut conn).expect("跑迁移");
        run_maintenance(&conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM clipboard_items", [], |row| row.get(0))
            .expect("维护后仍可查询");
        assert_eq!(count, 0);
    }
}
