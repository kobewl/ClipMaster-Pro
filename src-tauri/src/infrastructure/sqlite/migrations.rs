//! SQLite schema 与 migration。
//! 每个版本号对应一段幂等 SQL，只会被执行一次。

use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i64 = 3;

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
    // -----------------------------------------------------------------------
    // Migration 3: 改用 trigram tokenizer 支持 CJK 子串搜索。
    //
    // 默认 unicode61 tokenizer 以词为单位分词，对中文 "剪贴板管理" 只能
    // 匹配整个词，搜 "贴板" 两个字完全搜不到。
    //
    // trigram 以 3 字符滑动窗口切分，天然支持任意 ≥3 字符的子串匹配：
    //   "剪贴板管理" → ["剪贴板", "贴板管", "板管理"]
    //   搜索 "贴板管" → 命中
    //
    // 对于 < 3 字符的查询，repository 会 fallback 到 LIKE，性能可接受
    // （短查询本身匹配面很广，FTS 优势不大）。
    //
    // case_sensitive=0 让英文搜索不区分大小写。
    // -----------------------------------------------------------------------
    (
        3,
        r#"
    DROP TABLE IF EXISTS clipboard_items_fts;
    CREATE VIRTUAL TABLE clipboard_items_fts USING fts5(
        search_text,
        content=clipboard_items,
        content_rowid=rowid,
        tokenize='trigram case_sensitive 0'
    );
    INSERT INTO clipboard_items_fts(clipboard_items_fts) VALUES('rebuild');
    "#,
    ),
];

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
