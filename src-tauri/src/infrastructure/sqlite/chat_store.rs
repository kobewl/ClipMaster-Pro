//! 对话派生层。正文只存在这里，审计表仍然不存 prompt / 响应。
//!
//! 硬上限：最多 [`MAX_CHATS`] 段对话、每段 [`MAX_MESSAGES_PER_CHAT`] 条、
//! 单条 [`MAX_MESSAGE_CHARS`] 字。写入时裁剪，不靠后台任务，避免越用库越大。
//!
//! 与其它 store 一样，SQL 都在 `spawn_blocking` 里跑：共享连接的锁可能被别的
//! store 长时间占着，不能让 async worker 线程在这里同步等锁、同步跑 SQL。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::RepositoryError;
use crate::domain::model::{AgentChatDetail, AgentChatMessage, AgentChatSummary};
use crate::domain::ports::AgentChatStore;
use crate::infrastructure::sqlite::lock_conn;
use crate::infrastructure::sqlite::repository::parse_datetime;

pub const MAX_CHATS: u32 = 30;
pub const MAX_MESSAGES_PER_CHAT: u32 = 40;
pub const MAX_MESSAGE_CHARS: usize = 6_000;

pub struct SqliteChatStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteChatStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn map_err(err: rusqlite::Error) -> RepositoryError {
        RepositoryError::Database(err.to_string())
    }

    fn clip_content(content: &str) -> String {
        let count = content.chars().count();
        if count <= MAX_MESSAGE_CHARS {
            return content.to_string();
        }
        let clipped: String = content.chars().take(MAX_MESSAGE_CHARS).collect();
        format!("{clipped}…")
    }
}

#[async_trait]
impl AgentChatStore for SqliteChatStore {
    async fn insert(
        &self,
        id: &str,
        title: &str,
        created_at: &str,
        updated_at: &str,
    ) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        let id = id.to_string();
        let title = title.to_string();
        let created_at = created_at.to_string();
        let updated_at = updated_at.to_string();
        tokio::task::spawn_blocking(move || {
            let mut conn = lock_conn(&conn);
            let tx = conn.transaction().map_err(Self::map_err)?;
            tx.execute(
                "INSERT INTO agent_chats (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, title, created_at, updated_at],
            )
            .map_err(Self::map_err)?;
            tx.execute(
                "DELETE FROM agent_chats WHERE id IN (
                SELECT id FROM agent_chats ORDER BY updated_at DESC, rowid DESC
                LIMIT -1 OFFSET ?1
            )",
                params![MAX_CHATS],
            )
            .map_err(Self::map_err)?;
            tx.commit().map_err(Self::map_err)?;
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn append_message(
        &self,
        chat_id: &str,
        role: &str,
        content: &str,
        created_at: &str,
    ) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        let chat_id = chat_id.to_string();
        let role = role.to_string();
        let content = content.to_string();
        let created_at = created_at.to_string();
        tokio::task::spawn_blocking(move || {
            let mut conn = lock_conn(&conn);
            let tx = conn.transaction().map_err(Self::map_err)?;
            let next: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(position), 0) + 1 FROM agent_chat_messages WHERE chat_id = ?1",
                    params![chat_id],
                    |row| row.get(0),
                )
                .map_err(Self::map_err)?;
            let body = Self::clip_content(&content);
            tx.execute(
                "INSERT INTO agent_chat_messages (chat_id, position, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
                params![chat_id, next, role, body, created_at],
            )
            .map_err(Self::map_err)?;
            tx.execute(
                "DELETE FROM agent_chat_messages WHERE chat_id = ?1 AND position IN (
                SELECT position FROM agent_chat_messages
                WHERE chat_id = ?1
                ORDER BY position DESC
                LIMIT -1 OFFSET ?2
            )",
                params![chat_id, MAX_MESSAGES_PER_CHAT],
            )
            .map_err(Self::map_err)?;
            tx.commit().map_err(Self::map_err)?;
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn touch(
        &self,
        chat_id: &str,
        title: Option<&str>,
        updated_at: &str,
    ) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        let chat_id = chat_id.to_string();
        let title: Option<String> = title.map(str::to_string);
        let updated_at = updated_at.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            if let Some(title) = title {
                conn.execute(
                    "UPDATE agent_chats SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, updated_at, chat_id],
                )
                .map_err(Self::map_err)?;
            } else {
                conn.execute(
                    "UPDATE agent_chats SET updated_at = ?1 WHERE id = ?2",
                    params![updated_at, chat_id],
                )
                .map_err(Self::map_err)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn list(&self, limit: u32) -> Result<Vec<AgentChatSummary>, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            let mut stmt = conn
                .prepare(
                    "SELECT c.id, c.title, c.created_at, c.updated_at,
                        (SELECT COUNT(*) FROM agent_chat_messages m WHERE m.chat_id = c.id)
                 FROM agent_chats c
                 ORDER BY c.updated_at DESC, c.rowid DESC
                 LIMIT ?1",
                )
                .map_err(Self::map_err)?;
            let rows = stmt
                .query_map(params![limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })
                .map_err(Self::map_err)?;
            let mut out = Vec::new();
            for row in rows {
                let (id, title, created_at, updated_at, message_count) =
                    row.map_err(Self::map_err)?;
                out.push(AgentChatSummary {
                    id,
                    title,
                    created_at: parse_datetime(&created_at),
                    updated_at: parse_datetime(&updated_at),
                    message_count: message_count as u64,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn find(&self, id: &str) -> Result<Option<AgentChatDetail>, RepositoryError> {
        let conn = self.conn.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            let header = conn
                .query_row(
                    "SELECT id, title, created_at, updated_at FROM agent_chats WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(Self::map_err)?;
            let Some((id, title, created_at, updated_at)) = header else {
                return Ok(None);
            };
            let mut stmt = conn
                .prepare(
                    "SELECT role, content, created_at FROM agent_chat_messages
                 WHERE chat_id = ?1 ORDER BY position ASC",
                )
                .map_err(Self::map_err)?;
            let rows = stmt
                .query_map(params![id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(Self::map_err)?;
            let mut messages = Vec::new();
            for row in rows {
                let (role, content, created) = row.map_err(Self::map_err)?;
                messages.push(AgentChatMessage {
                    role,
                    content,
                    created_at: parse_datetime(&created),
                });
            }
            Ok(Some(AgentChatDetail {
                id,
                title,
                created_at: parse_datetime(&created_at),
                updated_at: parse_datetime(&updated_at),
                messages,
            }))
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let conn = self.conn.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            let changed = conn
                .execute("DELETE FROM agent_chats WHERE id = ?1", params![id])
                .map_err(Self::map_err)?;
            Ok(changed > 0)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn clear(&self) -> Result<u64, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            let changed = conn
                .execute("DELETE FROM agent_chats", [])
                .map_err(Self::map_err)?;
            Ok(changed as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}
