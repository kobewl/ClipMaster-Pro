//! SQLite schema 与 migration。

use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i64 = 11;

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
    // （第一条索引已由 migration 9 替换：列表不再分组置顶。）
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
    (
        8,
        r#"
    -- 对话是用户看得见的派生数据（和 Flow 会话同类），不是审计。
    -- 审计仍然不存 prompt / 响应正文；对话正文只活在这两张表里，
    -- 可被「清除所有 AI 派生数据」一键抹掉。
    --
    -- 表有硬上限（应用层写入时裁到 30 段 / 每段 40 条），避免越用磁盘越大。
    CREATE TABLE IF NOT EXISTS agent_chats (
        id         TEXT PRIMARY KEY,
        title      TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_agent_chats_updated_at
        ON agent_chats(updated_at DESC);

    CREATE TABLE IF NOT EXISTS agent_chat_messages (
        chat_id    TEXT NOT NULL REFERENCES agent_chats(id) ON DELETE CASCADE,
        position   INTEGER NOT NULL,
        role       TEXT NOT NULL CHECK(role IN ('user','assistant')),
        content    TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (chat_id, position)
    );
    "#,
    ),
    // 列表不再把已分组条目置顶，默认列表只按复制时间倒序：换成与新 ORDER BY 一致的索引。
    (
        9,
        r#"
    CREATE INDEX IF NOT EXISTS idx_clipboard_items_copied_time
        ON clipboard_items(last_copied_at DESC);
    DROP INDEX IF EXISTS idx_clipboard_items_list_order;
    "#,
    ),
    // 复制时记录来源应用的 bundle id 与 .app 路径，用于取真实图标；
    // 旧行保持 NULL，按应用名兜底。没有按它们查询的场景，不建索引。
    (
        10,
        r#"
    ALTER TABLE clipboard_items ADD COLUMN source_bundle_id TEXT;
    ALTER TABLE clipboard_items ADD COLUMN source_app_path TEXT;
    "#,
    ),
    // 删掉三个从没被查询用到的索引（dbstat 实测 search_text 索引约占主表 54%）：
    // - search_text：检索只有 `LIKE '%x%'` 与 FTS，b-tree 索引用不上；
    // - favorite_time：is_favorite 在 migration 4 已被 group_id 取代，新行恒为 0；
    // - legacy_id：只在 INSERT 时写入、读回模型，没有任何按它查找的路径。
    (
        11,
        r#"
    DROP INDEX IF EXISTS idx_clipboard_items_search_text;
    DROP INDEX IF EXISTS idx_clipboard_items_favorite_time;
    DROP INDEX IF EXISTS idx_clipboard_items_legacy_id;
    "#,
    ),
];

pub fn run_migrations(conn: &mut Connection) -> rusqlite::Result<()> {
    apply_migrations(conn, MIGRATIONS)
}

/// 按顺序应用 `migrations` 里尚未执行过的版本。拆出来是为了让测试能先把库
/// 停在某个旧版本，再验证升级到最新版本的路径。
fn apply_migrations(conn: &mut Connection, migrations: &[(i64, &str)]) -> rusqlite::Result<()> {
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
    for (version, sql) in migrations {
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

    /// 已经停在 v9 的库（真机升级的样子）跑完迁移后到达最新版本，旧行原样保留，
    /// 新增的 bundle 列为 NULL（图标按应用名兜底）。
    #[test]
    fn migration_10_applies_on_v9_database_and_preserves_rows() {
        use crate::infrastructure::sqlite::repository::SqliteClipboardRepository;

        assert_eq!(MIGRATIONS[8].0, 9, "切片 [..9] 必须正好停在 v9");
        let mut conn = Connection::open_in_memory().expect("内存库");
        apply_migrations(&mut conn, &MIGRATIONS[..9]).expect("迁移到 v9");

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("读版本号");
        assert_eq!(version, 9);
        let has_bundle_column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('clipboard_items') WHERE name = 'source_bundle_id'",
                [],
                |row| row.get(0),
            )
            .expect("读列信息");
        assert_eq!(has_bundle_column, 0, "v9 还没有 bundle 列");

        let id = "22222222-2222-2222-2222-222222222222";
        conn.execute(
            "INSERT INTO clipboard_items
                (id, content_type, content_text, fingerprint, search_text, is_favorite, group_id,
                 created_at, updated_at, last_copied_at, source_app, source_url, legacy_id)
             VALUES (?1, 'text', 'v9 时代的内容', 'fp-v9', 'v9 时代的内容', 0, NULL,
                     '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00',
                     '2026-01-01T00:00:00+00:00', 'Safari', 'https://example.com', NULL)",
            [id],
        )
        .expect("写入 v9 行");

        run_migrations(&mut conn).expect("升级到最新版本");

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("读版本号");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM clipboard_items", [], |row| row.get(0))
            .expect("数行数");
        assert_eq!(count, 1);

        let item = conn
            .query_row(
                "SELECT * FROM clipboard_items WHERE id = ?1",
                [id],
                SqliteClipboardRepository::row_to_item,
            )
            .expect("按统一口径读回旧行");
        assert_eq!(item.id.to_string(), id);
        assert_eq!(item.content_text, "v9 时代的内容");
        assert_eq!(item.source_app.as_deref(), Some("Safari"));
        assert_eq!(item.source_url.as_deref(), Some("https://example.com"));
        assert!(item.source_bundle_id.is_none());
        assert!(item.source_app_path.is_none());
    }

    const DROPPED_IN_11: [&str; 3] = [
        "idx_clipboard_items_search_text",
        "idx_clipboard_items_favorite_time",
        "idx_clipboard_items_legacy_id",
    ];

    fn index_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name IS NOT NULL")
            .expect("查索引");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("读索引");
        rows.collect::<Result<Vec<_>, _>>().expect("收集索引名")
    }

    /// 停在 v10 的库（真机升级的样子）跑完迁移 11：三个无用索引被删，
    /// 热路径用到的索引都还在，数据行原样保留。
    #[test]
    fn migration_11_drops_unused_indexes_on_v10_database() {
        assert_eq!(MIGRATIONS[9].0, 10, "切片 [..10] 必须正好停在 v10");
        let mut conn = Connection::open_in_memory().expect("内存库");
        apply_migrations(&mut conn, &MIGRATIONS[..10]).expect("迁移到 v10");

        let before = index_names(&conn);
        for name in DROPPED_IN_11 {
            assert!(
                before.iter().any(|n| n == name),
                "v10 应该还有 {name}：{before:?}"
            );
        }

        conn.execute(
            "INSERT INTO clipboard_items
                (id, content_type, content_text, fingerprint, search_text, is_favorite, group_id,
                 created_at, updated_at, last_copied_at, source_app, source_url, legacy_id)
             VALUES ('33333333-3333-3333-3333-333333333333', 'text', 'v10 时代的内容',
                     'fp-v10', 'v10 时代的内容', 0, NULL,
                     '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00',
                     '2026-01-01T00:00:00+00:00', 'Safari', NULL, NULL)",
            [],
        )
        .expect("写入 v10 行");

        run_migrations(&mut conn).expect("升级到最新版本");

        let after = index_names(&conn);
        for name in DROPPED_IN_11 {
            assert!(
                !after.iter().any(|n| n == name),
                "{name} 应该已被删除：{after:?}"
            );
        }
        for name in [
            "idx_clipboard_items_fingerprint",
            "idx_clipboard_items_copied_time",
            "idx_clipboard_items_group_time",
            "idx_clipboard_items_group_id",
        ] {
            assert!(
                after.iter().any(|n| n == name),
                "{name} 必须保留：{after:?}"
            );
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM clipboard_items", [], |row| row.get(0))
            .expect("数行数");
        assert_eq!(count, 1);
    }

    /// 仓储的真实查询形状在迁移 11 之后仍然走索引：不全表扫描、不为 ORDER BY
    /// 建临时 b-tree，也不依赖已删掉的索引。
    #[test]
    fn hot_queries_still_use_indexes_after_migration_11() {
        let mut conn = Connection::open_in_memory().expect("内存库");
        run_migrations(&mut conn).expect("跑迁移");

        let cases: [(&str, &str, bool); 5] = [
            (
                "列表",
                "SELECT clipboard_items.* FROM clipboard_items
                 ORDER BY clipboard_items.last_copied_at DESC LIMIT ?1 OFFSET ?2",
                true,
            ),
            (
                "时间筛选",
                "SELECT clipboard_items.* FROM clipboard_items
                 WHERE clipboard_items.last_copied_at >= ?1
                 ORDER BY clipboard_items.last_copied_at DESC LIMIT ?2 OFFSET ?3",
                true,
            ),
            (
                "分组视图",
                "SELECT clipboard_items.* FROM clipboard_items
                 WHERE clipboard_items.group_id = ?1
                 ORDER BY clipboard_items.last_copied_at DESC LIMIT ?2 OFFSET ?3",
                true,
            ),
            (
                "数量淘汰",
                "SELECT rowid, search_text, content_type, content_text
                 FROM clipboard_items
                 WHERE group_id IS NULL
                 ORDER BY last_copied_at ASC
                 LIMIT ?1",
                true,
            ),
            (
                "指纹去重",
                "SELECT id, search_text FROM clipboard_items WHERE fingerprint = ?1",
                false,
            ),
        ];

        for (label, sql, has_order_by) in cases {
            let mut stmt = conn
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .expect("EXPLAIN");
            // 参数值不影响计划，全部绑 NULL。
            let nulls = vec![rusqlite::types::Null; stmt.parameter_count()];
            let details: Vec<String> = stmt
                .query_map(rusqlite::params_from_iter(nulls), |row| {
                    row.get::<_, String>(3)
                })
                .expect("读计划")
                .collect::<Result<_, _>>()
                .expect("收集计划");
            let plan = details.join(" | ");
            assert!(
                plan.contains("idx_clipboard_items_"),
                "{label} 必须走索引：{plan}"
            );
            if has_order_by {
                assert!(
                    !plan.contains("USE TEMP B-TREE FOR ORDER BY"),
                    "{label} 的排序必须由索引给出：{plan}"
                );
            }
            for name in DROPPED_IN_11 {
                assert!(
                    !plan.contains(name),
                    "{label} 不能依赖已删除的 {name}：{plan}"
                );
            }
        }
    }
}
