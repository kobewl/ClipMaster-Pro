//! SQLite 实现的 AgentSessionStore（Flow 会话派生层）。
//!
//! 三条不变量：
//! 1. **会话不悬空** —— 成员条目被删（用户删除 / 清空 / 按条数淘汰 / 按天过期，
//!    四条路径）到一条不剩时，会话本身由 migration 7 的触发器带走。其中两条是
//!    自动跑的，用户看不见，所以这条不能靠调用方自觉。
//! 2. **重算只换 agent** —— `source='agent'` 的会话由一次重算整体替换（单事务），
//!    `source='user'` 的会话既不被替换、也不被自动裁剪，只有用户自己删
//!    或它的条目全没了才会消失。
//! 3. **`position` 从 1 开始** —— 界面上的编号就是库里的编号。schema 里刻意没有
//!    这条 CHECK（契约真相源优先，事后补列要重建表），而 store 是成员行唯一的
//!    写入者，所以这里是那道兜底闸门（见 `ensure_position`）。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::RepositoryError;
use crate::domain::model::{
    AgentSessionDetail, AgentSessionDraft, AgentSessionMember, AgentSessionSummary, SessionSource,
};
use crate::domain::ports::AgentSessionStore;
use crate::infrastructure::sqlite::repository::{parse_datetime, SqliteClipboardRepository};

/// 保留的 agent 源会话条数上限。
///
/// 量级理由：候选窗口是「最近 3 天 ∩ 最近 200 条」（Task 3 的 `SESSION_SCAN_LIMIT`
/// 与它同量级），而一个会话至少 2 条内容 —— 200 / 2 = 100，就是一次重算理论上
/// 能产出的会话数上界。裁剪只针对 `source='agent'`（照 `agent_run_store::MAX_RUNS`
/// 的写法：表有上界，不留后台清理任务）。
pub const MAX_SESSIONS: u32 = 100;

pub struct SqliteSessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSessionStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn map_db_err(err: rusqlite::Error) -> RepositoryError {
        RepositoryError::Database(err.to_string())
    }
}

/// 成员行的来源。CHECK 保证只有 `user` / `agent`；真读出别的值说明库被人改过，
/// 退回 `agent`（要求理由、会被重算替换的那一侧）比「装作无事」更保守。
fn session_source(value: String) -> SessionSource {
    SessionSource::from_db_str(&value).unwrap_or(SessionSource::Agent)
}

/// `position` 闸门（模块头第 3 条）：越界就是调用方的 bug，当场拒绝而不是落库。
///
/// 错误类型用 `RepositoryError::Database` 而不是新造一个码：`position <= 0` 在
/// 设计上**不可达**（`session_build` 永远从 1 开始编号），真撞上说明是应用层
/// 代码写错了 —— 这属于内部 bug，不是用户可以自救的路径，所以不值得为它单开
/// 一个面向用户的错误码。经命令层最终会映射成 `ai_session_store_unavailable`
/// （「无法读写本地会话数据」），措辞上偏保守但没有把内部 bug 伪装成用户问题。
/// 是否要为它单独成类（例如 `ai_session_items_invalid`），留给 Task 4 命令层定夺。
fn ensure_position(position: i64, item_id: &str) -> Result<(), RepositoryError> {
    if position <= 0 {
        return Err(RepositoryError::Database(format!(
            "会话成员的 position 必须从 1 开始（收到 {position}，item_id={item_id}）"
        )));
    }
    Ok(())
}

/// 写会话行时撞到同 id 的处理策略。
///
/// 为什么需要两种：会话 id 由成员集合派生（关键判断 4），所以「同一个 id」
/// 一定会出现 —— 只是出现的路径不同，而两条路径的正确答案相反。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Conflict {
    /// 用户路径：同一批成员重复保存 = 同一个 id = 「还是那个会话」，按覆盖处理。
    /// 用户点两次保存不是两次意图，第二次只是把同一个会话再写一遍。
    Overwrite,
    /// agent 重算路径：让位给已存在的那条。
    ///
    /// 重算前刚删光全部 `source='agent'` 行，此处的同 id 冲突**只可能**是用户
    /// 手存的会话（计划第 5 条「`source='user'` 永不参与替换」的延伸）。
    /// 若在这里覆盖或报错，用户手动保存的恰恰是自动聚类会分出的那一组 ——
    /// 最自然的用户行为 —— 会让整笔重算事务回滚、工作台每次打开都失败。
    ///
    /// 同一批里出现重复 id 时也是「先到先得」（`build` 产出的 id 由互斥成员
    /// 集合派生，批内重复不可达；真出现时跳过比覆盖更保守）。
    Skip,
}

/// 写一条会话 + 它的成员行。调用方负责事务 —— 重算（整体替换）与用户手动保存
/// 共用这一条写入路径，两条路径的口径不会分叉；**冲突策略**是唯一的分叉点，
/// 由调用方按上面 `Conflict` 的语义选。
///
/// 返回是否真的写入了（`Skip` 撞到同 id 时返回 `false`）。
fn write_session(
    tx: &rusqlite::Transaction<'_>,
    session: &AgentSessionDraft,
    conflict: Conflict,
) -> Result<bool, RepositoryError> {
    if conflict == Conflict::Overwrite {
        // 显式「先删同 id 再写」，而不是 `INSERT OR REPLACE`：
        // 覆盖的真实脆点是**成员行残留** —— 旧行的成员必须跟着旧行一起走，
        // 而这靠的是外键级联（显式 DELETE 一定触发）。REPLACE 走的是内部的
        // 删行路径，是否触发 delete 触发器取决于 `recursive_triggers` pragma
        // （当前构建实测两者行为等价，但那是环境事实、不是语句保证）。
        // 显式删除让时序与预期一致，不把「新旧行谁活下来」押在一个 pragma
        // 的默认值上。
        tx.execute(
            "DELETE FROM agent_sessions WHERE id = ?1",
            params![session.id],
        )
        .map_err(SqliteSessionStore::map_db_err)?;
    }

    // 冲突跳过：agent 重算产出同 id 时，库里那条是用户手存的会话 ——
    // 一个字都不动（含成员与理由），本次写入整条放弃。
    if conflict == Conflict::Skip
        && tx
            .query_row(
                "SELECT 1 FROM agent_sessions WHERE id = ?1",
                params![session.id],
                |_| Ok(()),
            )
            .optional()
            .map_err(SqliteSessionStore::map_db_err)?
            .is_some()
    {
        return Ok(false);
    }

    tx.execute(
        "INSERT INTO agent_sessions (id, source, title, summary, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session.id,
            session.source.as_str(),
            session.title,
            session.summary,
            session.created_at.to_rfc3339(),
            session.updated_at.to_rfc3339(),
        ],
    )
    .map_err(SqliteSessionStore::map_db_err)?;

    let added_at = Utc::now().to_rfc3339();
    for member in &session.items {
        ensure_position(member.position, &member.item_id)?;
        // 成员行的 source 取自所属会话（不设第二个真相源）；reason 原样写入 ——
        // 「agent 源理由非空」由数据库的条件 CHECK 兜底，这里不替它编一个理由，
        // 于是也不用担心两条写入路径给出不同口径的理由。
        tx.execute(
            "INSERT INTO agent_session_items
                 (session_id, item_id, position, source, reason, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id,
                member.item_id,
                member.position,
                session.source.as_str(),
                member.reason,
                added_at,
            ],
        )
        .map_err(SqliteSessionStore::map_db_err)?;
    }
    Ok(true)
}

/// 写入后把 agent 源会话裁到 [`MAX_SESSIONS`]。
///
/// 只裁 `source='agent'`：`source='user'` 是用户亲手存的东西，自动淘汰它等于
/// 替用户做决定。排序与列表接口同一口径（`updated_at DESC, rowid DESC`），
/// 同一时刻写入的多条因此有稳定序。删会话会级联带走成员行。
fn trim_agent_sessions(tx: &rusqlite::Transaction<'_>) -> Result<(), RepositoryError> {
    tx.execute(
        "DELETE FROM agent_sessions
         WHERE source = 'agent'
           AND id NOT IN (
               SELECT id FROM agent_sessions
               WHERE source = 'agent'
               ORDER BY updated_at DESC, rowid DESC
               LIMIT ?1
           )",
        params![MAX_SESSIONS],
    )
    .map_err(SqliteSessionStore::map_db_err)?;
    Ok(())
}

#[async_trait]
impl AgentSessionStore for SqliteSessionStore {
    async fn replace_agent_sessions(
        &self,
        sessions: Vec<AgentSessionDraft>,
    ) -> Result<u64, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(SqliteSessionStore::map_db_err)?;

            // 先删后写，都在同一个事务里：会话 id 由成员集合派生（关键判断 4），
            // 未变化的会话在替换前后逐字段相等，所以「重算幂等」是可断言的；
            // 中途出错则整笔回滚，库里的旧会话不会因为半途失败而消失。
            // 只删 source='agent'：用户手动保存的会话永不被重算带走。
            tx.execute("DELETE FROM agent_sessions WHERE source = 'agent'", [])
                .map_err(SqliteSessionStore::map_db_err)?;

            // 与用户手存会话同 id 的那条**跳过**（让位给用户）：见 `Conflict::Skip`。
            let mut written: u64 = 0;
            for session in &sessions {
                if write_session(&tx, session, Conflict::Skip)? {
                    written += 1;
                }
            }
            trim_agent_sessions(&tx)?;

            tx.commit().map_err(SqliteSessionStore::map_db_err)?;
            Ok(written)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn insert_session(&self, session: AgentSessionDraft) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(SqliteSessionStore::map_db_err)?;

            // 用户路径：同 id 是「还是那个会话」，覆盖写（见 `Conflict::Overwrite`）。
            write_session(&tx, &session, Conflict::Overwrite)?;
            trim_agent_sessions(&tx)?;

            tx.commit().map_err(SqliteSessionStore::map_db_err)?;
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn list(&self, limit: u32) -> Result<Vec<AgentSessionSummary>, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");

            // 成员计数走相关子查询：`PRIMARY KEY(session_id, item_id)` 的前缀索引
            // 直接覆盖，比 JOIN + GROUP BY 少一趟聚合。会话表本身有上界
            // （MAX_SESSIONS），所以这里不做分页。
            // ORDER BY 与 idx_agent_sessions_updated_at 同序，rowid DESC 让同一
            // 时刻写入的多条有稳定序（照 agent_run_store 的列表写法）。
            let mut stmt = conn
                .prepare(
                    "SELECT s.id, s.source, s.title, s.summary, s.created_at, s.updated_at,
                            (SELECT COUNT(*) FROM agent_session_items i
                              WHERE i.session_id = s.id) AS item_count
                     FROM agent_sessions s
                     ORDER BY s.updated_at DESC, s.rowid DESC
                     LIMIT ?1",
                )
                .map_err(SqliteSessionStore::map_db_err)?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok(AgentSessionSummary {
                        id: row.get("id")?,
                        source: session_source(row.get::<_, String>("source")?),
                        title: row.get("title")?,
                        summary: row.get("summary")?,
                        created_at: parse_datetime(&row.get::<_, String>("created_at")?),
                        updated_at: parse_datetime(&row.get::<_, String>("updated_at")?),
                        item_count: row.get::<_, i64>("item_count")?.max(0) as u64,
                    })
                })
                .map_err(SqliteSessionStore::map_db_err)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(SqliteSessionStore::map_db_err)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn find(&self, id: &str) -> Result<Option<AgentSessionDetail>, RepositoryError> {
        let conn = self.conn.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");

            let session = conn
                .query_row(
                    "SELECT id, source, title, summary, created_at, updated_at
                     FROM agent_sessions WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>("id")?,
                            row.get::<_, String>("source")?,
                            row.get::<_, String>("title")?,
                            row.get::<_, String>("summary")?,
                            row.get::<_, String>("created_at")?,
                            row.get::<_, String>("updated_at")?,
                        ))
                    },
                )
                .optional()
                .map_err(SqliteSessionStore::map_db_err)?;
            let Some((id, source, title, summary, created_at, updated_at)) = session else {
                return Ok(None);
            };

            // 条目行走 `SqliteClipboardRepository::row_to_item`：会话详情里的条目
            // 必须与历史列表按同一口径映射，两份 row 映射迟早会漂移。
            // ORDER BY 里的 rowid 是平局时的稳定序（position 理论上是唯一的，
            // 但库被人改过时不至于让同一个会话两次读出不同顺序）。
            let mut stmt = conn
                .prepare(
                    "SELECT i.position, i.reason, c.*
                     FROM agent_session_items i
                     INNER JOIN clipboard_items c ON c.id = i.item_id
                     WHERE i.session_id = ?1
                     ORDER BY i.position ASC, i.rowid ASC",
                )
                .map_err(SqliteSessionStore::map_db_err)?;
            let rows = stmt
                .query_map(params![id], |row| {
                    Ok(AgentSessionMember {
                        item: SqliteClipboardRepository::row_to_item(row)?,
                        position: row.get("position")?,
                        reason: row.get("reason")?,
                    })
                })
                .map_err(SqliteSessionStore::map_db_err)?;
            let members = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(SqliteSessionStore::map_db_err)?;

            Ok(Some(AgentSessionDetail {
                id,
                source: session_source(source),
                title,
                summary,
                created_at: parse_datetime(&created_at),
                updated_at: parse_datetime(&updated_at),
                members,
            }))
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let conn = self.conn.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            // 成员行由外键 ON DELETE CASCADE 带走，不手写第二条 DELETE。
            let affected = conn
                .execute("DELETE FROM agent_sessions WHERE id = ?1", params![id])
                .map_err(SqliteSessionStore::map_db_err)?;
            Ok(affected > 0)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn clear(&self) -> Result<u64, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let affected = conn
                .execute("DELETE FROM agent_sessions", [])
                .map_err(SqliteSessionStore::map_db_err)?;
            Ok(affected as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `session_source` 的退路语义：只有库里被人改出第三个值时才会走到
    /// 认不出的分支，此时退回 `Agent`（要求理由、会被重算替换的那一侧）
    /// 比「装作无事」保守 —— 一条来源不明的成员行不该被当成用户亲手存的，
    /// 那会让它永远躲过重算。
    #[test]
    fn unknown_source_strings_fall_back_to_agent() {
        assert_eq!(session_source("user".to_string()), SessionSource::User);
        assert_eq!(session_source("agent".to_string()), SessionSource::Agent);
        assert_eq!(
            session_source("robot".to_string()),
            SessionSource::Agent,
            "认不出的来源退回 Agent（保守侧），而不是 User"
        );
        assert_eq!(
            session_source(String::new()),
            SessionSource::Agent,
            "空串同样退回 Agent"
        );
    }
}
