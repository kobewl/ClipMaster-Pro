//! SQLite schema 与 migration。
//!
//! Migration 策略：每个版本号对应一段幂等 SQL，只会被执行一次。
//! 新功能通过追加新版本号来演进，不修改已有 migration。

use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i64 = 2;

const MIGRATIONS: &[(i64, &str)] = &[
    (
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
    ),
    // -----------------------------------------------------------------------
    // Migration 2: 引入 FTS5 全文搜索，取代 LIKE '%xxx%'。
    //
    // FTS5 虚拟表用 external content 模式（content=clipboard_items），
    // 这样 FTS 索引不会拷贝一份完整的 content_text，只存倒排索引，
    // 查询时通过 rowid → clipboard_items 回表取完整数据。
    //
    // 对于老用户（已有 migration 1 的数据），通过 rebuild 命令一次性
    // 把已有行灌进 FTS 索引；新用户第一次运行 rebuild 是空表，无开销。
    // -----------------------------------------------------------------------
    (
        2,
        r#"
    CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_items_fts USING fts5(
        search_text,
        content=clipboard_items,
        content_rowid=rowid
    );

    INSERT INTO clipboard_items_fts(clipboard_items_fts) VALUES('rebuild');
    "#,
    ),
];

/// 执行全部尚未应用的 migration。若任一步骤失败，整体回滚。
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
