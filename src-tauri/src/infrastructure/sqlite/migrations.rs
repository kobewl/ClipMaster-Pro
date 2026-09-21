//! SQLite schema 与 migration。

use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i64 = 6;

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
    // -----------------------------------------------------------------------
    // Migration 4: 分组系统（替代布尔收藏）+ 来源 URL
    // -----------------------------------------------------------------------
    (
        4,
        r#"
    CREATE TABLE IF NOT EXISTS clip_groups (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        color       TEXT NOT NULL DEFAULT '#3B82F6',
        sort_order  INTEGER NOT NULL DEFAULT 0,
        created_at  TEXT NOT NULL,
        updated_at  TEXT NOT NULL
    );

    ALTER TABLE clipboard_items ADD COLUMN group_id TEXT;
    ALTER TABLE clipboard_items ADD COLUMN source_url TEXT;

    CREATE INDEX IF NOT EXISTS idx_clipboard_items_group_id
        ON clipboard_items(group_id);

    -- 把现有收藏项迁移到默认"收藏"分组
    INSERT INTO clip_groups (id, name, color, sort_order, created_at, updated_at)
    SELECT
        '00000000-0000-0000-0000-000000000001',
        '收藏', '#F59E0B', 0,
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE EXISTS (SELECT 1 FROM clipboard_items WHERE is_favorite = 1);

    UPDATE clipboard_items
    SET group_id = '00000000-0000-0000-0000-000000000001'
    WHERE is_favorite = 1;
    "#,
    ),
    // 列表默认先展示已分组条目、再按最后复制时间倒排。用与 ORDER BY 完全一致的
    // 表达式索引，避免历史数量增长后每次刷新都临时排序整张表；分组视图则使用
    // group_id + 时间的复合索引直接取得第一页。
    (
        5,
        r#"
    CREATE INDEX IF NOT EXISTS idx_clipboard_items_list_order
        ON clipboard_items((group_id IS NULL), last_copied_at DESC);
    CREATE INDEX IF NOT EXISTS idx_clipboard_items_group_time
        ON clipboard_items(group_id, last_copied_at DESC);
    "#,
    ),
    // -----------------------------------------------------------------------
    // Migration 6: AI 调用审计（Phase 1 第 1 步）
    //
    // 只存元数据：prompt 正文和模型响应正文都在剪贴板历史里，审计再存一份
    // 等于把隐私面翻倍。密钥本来就只在内存里流转，不落表。
    // -----------------------------------------------------------------------
    (
        6,
        r#"
    CREATE TABLE IF NOT EXISTS agent_runs (
        id           TEXT PRIMARY KEY,
        created_at   TEXT NOT NULL,
        action       TEXT NOT NULL,
        provider     TEXT,
        model        TEXT,
        input_chars  INTEGER NOT NULL DEFAULT 0,
        status       TEXT NOT NULL,
        error_code   TEXT,
        duration_ms  INTEGER NOT NULL DEFAULT 0,
        output_chars INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_agent_runs_created_at
        ON agent_runs(created_at DESC);

    -- 一次调用用了哪些条目。单独建表而不是在 agent_runs 里塞 JSON 数组，
    -- 就是为了让 FOREIGN KEY 真正生效：条目被删时关联行自动消失。
    CREATE TABLE IF NOT EXISTS agent_run_items (
        run_id  TEXT NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
        item_id TEXT NOT NULL REFERENCES clipboard_items(id) ON DELETE CASCADE,
        PRIMARY KEY (run_id, item_id)
    );
    CREATE INDEX IF NOT EXISTS idx_agent_run_items_item
        ON agent_run_items(item_id);

    -- 主语全没了，这条审计记录就无从追溯，一并删掉。做成触发器而不是在各处
    -- 手写清理：删除条目的路径有四条（用户删除 / 清空 / 按条数淘汰 / 按天过期），
    -- 触发器让「审计不悬空」成为数据库不变量，将来新增删除路径也不会漏。
    CREATE TRIGGER IF NOT EXISTS trg_agent_runs_drop_orphans
    AFTER DELETE ON agent_run_items
    BEGIN
        DELETE FROM agent_runs
        WHERE id = OLD.run_id
          AND NOT EXISTS (SELECT 1 FROM agent_run_items WHERE run_id = OLD.run_id);
    END;
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
