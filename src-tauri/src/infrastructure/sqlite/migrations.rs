//! SQLite schema 与 migration。
//!
//! 参考数据文档第 3.1、3.2 节：
//! - `schema_migrations` 记录已执行 migration 版本，应用启动时先确认结构可识别再启动写入。
//! - `clipboard_items` 主表结构见下方 SQL。
//! - `app_settings` 为 key-value 存储，承载最大历史数量、保留天数等设置（架构文档未强制要求
//!   独立表，这里选择表而非配置文件，方便与历史数据在同一事务边界内操作）。

use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i64 = 1;

/// 按顺序执行的 migration SQL。每个 migration 必须是幂等或受版本号保护的。
const MIGRATIONS: &[(i64, &str)] = &[(
    1,
    r#"
    CREATE TABLE IF NOT EXISTS clipboard_items (
        id              TEXT PRIMARY KEY,
        content_type    TEXT NOT NULL,
        content_text    TEXT NOT NULL,
        fingerprint     TEXT NOT NULL,
        search_text     TEXT NOT NULL DEFAULT '',
        is_favorite     INTEGER NOT NULL DEFAULT 0,
        created_at      TEXT NOT NULL,
        updated_at      TEXT NOT NULL,
        last_copied_at  TEXT NOT NULL,
        source_app      TEXT,
        legacy_id       TEXT
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_items_fingerprint
        ON clipboard_items(fingerprint);

    CREATE INDEX IF NOT EXISTS idx_clipboard_items_favorite_time
        ON clipboard_items(is_favorite, last_copied_at DESC);

    CREATE INDEX IF NOT EXISTS idx_clipboard_items_search_text
        ON clipboard_items(search_text);

    CREATE INDEX IF NOT EXISTS idx_clipboard_items_legacy_id
        ON clipboard_items(legacy_id);

    CREATE TABLE IF NOT EXISTS app_settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
)];

/// 执行全部尚未应用的 migration。若任一步骤失败，整体回滚，不允许应用
/// 在半升级结构上继续运行（数据文档第 5 节事务规则）。
pub fn run_migrations(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
        [],
    )?;

    let tx = conn.transaction()?;
    for (version, sql) in MIGRATIONS {
        let already_applied: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if already_applied {
            continue;
        }
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, chrono::Utc::now().to_rfc3339()],
        )?;
    }
    tx.commit()?;
    Ok(())
}
