//! SQLite schema 与 migration。

use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i64 = 7;

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
    // -----------------------------------------------------------------------
    // Migration 7: Flow 会话派生层（Phase 1 第 3 步）
    //
    // 会话是**派生数据**：不存 prompt、不存模型输出，全部由本地的
    // last_copied_at / source_app / 内容词推出（见 docs/agent/PHASE1_PLAN.md 主题 3）。
    //
    // 为什么 summary 是 NOT NULL：摘要是本地拼接的（时段 / 条数 / 来源 / 共享词），
    // 打开工作台即得，不存在「还没归纳所以为空」的中间状态 —— 允许 NULL 只会让
    // 界面多一条「为什么这里是空的」的猜谜。
    //
    // 为什么 source 落在两张表上：会话与成员各一行（将来「把某条手动塞进另一个
    // 会话」时成员来源可独立），写入时由 store 统一赋同一个值。
    //
    // 为什么 reason 用条件 CHECK：只有 agent 源必须「说得清为什么成群」（不做
    // 聪明的自动关联，凡自动成组必须有理由）；用户亲手保存的会话不需要理由 ——
    // 他自己知道为什么。条件写在 CHECK 里而不是分两个列，是为了让「agent 源
    // 理由非空」成为数据库不变量，应用层漏判也写不进脏数据。
    // -----------------------------------------------------------------------
    (
        7,
        r#"
    CREATE TABLE IF NOT EXISTS agent_sessions (
        id         TEXT PRIMARY KEY,
        source     TEXT NOT NULL CHECK(source IN ('user','agent')),
        title      TEXT NOT NULL,
        summary    TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_agent_sessions_updated_at
        ON agent_sessions(updated_at DESC);

    -- 一次会话用了哪些条目。单独建表而不是在 agent_sessions 里塞 JSON 数组，
    -- 就是为了让 FOREIGN KEY 真正生效：条目被删时成员行自动消失。
    -- position 从 1 开始：界面上的编号就是库里的编号，省掉所有 +1/-1 换算。
    CREATE TABLE IF NOT EXISTS agent_session_items (
        session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
        item_id    TEXT NOT NULL REFERENCES clipboard_items(id) ON DELETE CASCADE,
        position   INTEGER NOT NULL,
        source     TEXT NOT NULL CHECK(source IN ('user','agent')),
        reason     TEXT CHECK (source <> 'agent' OR (reason IS NOT NULL AND length(trim(reason)) > 0)),
        added_at   TEXT NOT NULL,
        PRIMARY KEY (session_id, item_id)
    );
    CREATE INDEX IF NOT EXISTS idx_agent_session_items_item
        ON agent_session_items(item_id);

    -- 最后一个成员也没了，这个会话就无从解释、也没有价值，一并删掉。做成触发器
    -- 而不是在各处手写清理：删除条目的路径有四条（用户删除 / 清空 / 按条数淘汰 /
    -- 按天过期），其中两条是自动跑的，用户看不见 —— 触发器让「会话不悬空」成为
    -- 数据库不变量，将来新增删除路径也不会漏（照 trg_agent_runs_drop_orphans 同形）。
    CREATE TRIGGER IF NOT EXISTS trg_agent_sessions_drop_orphans
    AFTER DELETE ON agent_session_items
    BEGIN
        DELETE FROM agent_sessions
        WHERE id = OLD.session_id
          AND NOT EXISTS (SELECT 1 FROM agent_session_items WHERE session_id = OLD.session_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 版本号必须严格递增且以 `CURRENT_SCHEMA_VERSION` 收尾。
    ///
    /// 写重复的版本号会让 `run_migrations` 静默跳过后面那一条（`schema_migrations`
    /// 一行一个版本），建表语句就此永远不执行，而库里的版本号看着一切正常 ——
    /// 这种错在真机上升级时才炸，所以在这里钉住。`MIGRATIONS` 是私有的，
    /// 只能在模块内断言。
    #[test]
    fn migrations_are_strictly_increasing_and_cover_the_declared_version() {
        for pair in MIGRATIONS.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "迁移版本必须严格递增：{} 后面又出现了 {}",
                pair[0].0,
                pair[1].0
            );
        }
        assert_eq!(
            MIGRATIONS.last().map(|(version, _)| *version),
            Some(CURRENT_SCHEMA_VERSION),
            "最后一条迁移的版本号必须等于 CURRENT_SCHEMA_VERSION，否则全新的库拿不到最新 schema"
        );
    }

    /// `CURRENT_SCHEMA_VERSION` 必须有真实的读取点：空库跑完迁移后，
    /// 库里的版本号就该等于这个常量。
    #[test]
    fn fresh_database_reaches_the_declared_version() {
        let mut conn = Connection::open_in_memory().expect("内存库");
        run_migrations(&mut conn).expect("跑迁移");

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("读版本号");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }
}
