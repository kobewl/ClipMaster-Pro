//! SQLite 实现的 AgentRunStore（AI 调用审计）。
//!
//! 两条不变量：
//! 1. **审计不悬空** —— 每条记录至少关联一条仍存在的剪贴板条目。条目被删
//!    （用户删除 / 清空 / 按条数淘汰 / 按天过期，四条路径）后由 migration 6
//!    里的触发器连带删掉记录本身。其中"按条数淘汰"和"按天过期"是自动跑的，
//!    用户看不见，所以这条不能靠调用方自觉。
//! 2. **表有上界** —— 每次写入顺手裁到 [`MAX_RUNS`] 条，不做后台清理任务。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::RepositoryError;
use crate::domain::ports::{AgentRunRecord, AgentRunStore};

/// 保留的审计条数上限。审计是"最近发生了什么"的时间线，不是归档。
const MAX_RUNS: u32 = 500;

pub struct SqliteAgentRunStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteAgentRunStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn map_db_err(err: rusqlite::Error) -> RepositoryError {
        RepositoryError::Database(err.to_string())
    }
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRunRecord> {
    Ok(AgentRunRecord {
        id: row.get("id")?,
        created_at: row.get("created_at")?,
        action: row.get("action")?,
        provider: row.get("provider")?,
        model: row.get("model")?,
        input_item_ids: Vec::new(),
        input_chars: row.get::<_, i64>("input_chars")?.max(0) as u64,
        status: row.get("status")?,
        error_code: row.get("error_code")?,
        duration_ms: row.get::<_, i64>("duration_ms")?.max(0) as u64,
        output_chars: row.get::<_, Option<i64>>("output_chars")?.map(|v| v.max(0) as u64),
    })
}

#[async_trait]
impl AgentRunStore for SqliteAgentRunStore {
    async fn record(&self, run: AgentRunRecord) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(SqliteAgentRunStore::map_db_err)?;

            tx.execute(
                "INSERT INTO agent_runs
                    (id, created_at, action, provider, model, input_chars,
                     status, error_code, duration_ms, output_chars)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    run.id,
                    run.created_at,
                    run.action,
                    run.provider,
                    run.model,
                    run.input_chars as i64,
                    run.status,
                    run.error_code,
                    run.duration_ms as i64,
                    run.output_chars.map(|v| v as i64),
                ],
            )
            .map_err(SqliteAgentRunStore::map_db_err)?;

            for item_id in &run.input_item_ids {
                // SELECT 而不是 VALUES：条目可能在请求途中被删掉，这条关联就该落空，
                // 而不是撞上外键约束把整条审计写失败。
                tx.execute(
                    "INSERT OR IGNORE INTO agent_run_items (run_id, item_id)
                     SELECT ?1, id FROM clipboard_items WHERE id = ?2",
                    params![run.id, item_id],
                )
                .map_err(SqliteAgentRunStore::map_db_err)?;
            }

            // 没有主语的记录没有追溯价值（条目压根不存在时就是这种），不留。
            tx.execute(
                "DELETE FROM agent_runs
                 WHERE NOT EXISTS (
                     SELECT 1 FROM agent_run_items WHERE run_id = agent_runs.id
                 )",
                [],
            )
            .map_err(SqliteAgentRunStore::map_db_err)?;

            tx.execute(
                "DELETE FROM agent_runs WHERE id IN (
                     SELECT id FROM agent_runs
                     ORDER BY created_at DESC, rowid DESC
                     LIMIT -1 OFFSET ?1
                 )",
                params![MAX_RUNS],
            )
            .map_err(SqliteAgentRunStore::map_db_err)?;

            tx.commit().map_err(SqliteAgentRunStore::map_db_err)?;
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn list_recent(&self, limit: u32) -> Result<Vec<AgentRunRecord>, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");

            let mut runs: Vec<AgentRunRecord> = {
                let mut stmt = conn
                    .prepare(
                        "SELECT * FROM agent_runs
                         ORDER BY created_at DESC, rowid DESC
                         LIMIT ?1",
                    )
                    .map_err(SqliteAgentRunStore::map_db_err)?;
                let rows = stmt
                    .query_map(params![limit], row_to_run)
                    .map_err(SqliteAgentRunStore::map_db_err)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(SqliteAgentRunStore::map_db_err)?
            };

            // 关联条目单独一趟取回，避免 JOIN 出行后还要在主查询里做聚合。
            let mut item_ids: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            {
                let mut stmt = conn
                    .prepare(
                        "SELECT run_id, item_id FROM agent_run_items
                         WHERE run_id IN (
                             SELECT id FROM agent_runs
                             ORDER BY created_at DESC, rowid DESC
                             LIMIT ?1
                         )",
                    )
                    .map_err(SqliteAgentRunStore::map_db_err)?;
                let rows = stmt
                    .query_map(params![limit], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(SqliteAgentRunStore::map_db_err)?;
                for row in rows {
                    let (run_id, item_id) = row.map_err(SqliteAgentRunStore::map_db_err)?;
                    item_ids.entry(run_id).or_default().push(item_id);
                }
            }

            for run in &mut runs {
                if let Some(ids) = item_ids.remove(&run.id) {
                    run.input_item_ids = ids;
                }
            }
            Ok(runs)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn find(&self, id: &str) -> Result<Option<AgentRunRecord>, RepositoryError> {
        let conn = self.conn.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");

            let mut run: Option<AgentRunRecord> = conn
                .query_row("SELECT * FROM agent_runs WHERE id = ?1", params![id], row_to_run)
                .optional()
                .map_err(SqliteAgentRunStore::map_db_err)?;

            let Some(run) = run.as_mut() else {
                return Ok(None);
            };

            let mut stmt = conn
                .prepare("SELECT item_id FROM agent_run_items WHERE run_id = ?1")
                .map_err(SqliteAgentRunStore::map_db_err)?;
            let rows = stmt
                .query_map(params![run.id], |row| row.get::<_, String>(0))
                .map_err(SqliteAgentRunStore::map_db_err)?;
            run.input_item_ids = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(SqliteAgentRunStore::map_db_err)?;
            Ok(Some(run.clone()))
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn clear(&self) -> Result<u64, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let affected = conn
                .execute("DELETE FROM agent_runs", [])
                .map_err(SqliteAgentRunStore::map_db_err)?;
            Ok(affected as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}
