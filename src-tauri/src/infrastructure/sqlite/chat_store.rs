//! 对话派生层。正文只存在这里，审计表仍然不存 prompt / 响应。
//!
//! 硬上限：最多 [`MAX_CHATS`] 段对话、每段 [`MAX_MESSAGES_PER_CHAT`] 条、
//! 单条 [`MAX_MESSAGE_CHARS`] 字。写入时裁剪，不靠后台任务，避免越用库越大。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::RepositoryError;
use crate::domain::model::{AgentChatDetail, AgentChatMessage, AgentChatSummary};
use crate::domain::ports::AgentChatStore;
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
        let mut conn = self.conn.lock().map_err(|err| {
            RepositoryError::Database(format!("对话库锁中毒: {err}"))
        })?;
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
    }

    async fn append_message(
        &self,
        chat_id: &str,
        role: &str,
        content: &str,
        created_at: &str,
    ) -> Result<(), RepositoryError> {
        let mut conn = self.conn.lock().map_err(|err| {
            RepositoryError::Database(format!("对话库锁中毒: {err}"))
        })?;
        let tx = conn.transaction().map_err(Self::map_err)?;
        let next: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(position), 0) + 1 FROM agent_chat_messages WHERE chat_id = ?1",
                params![chat_id],
                |row| row.get(0),
            )
            .map_err(Self::map_err)?;
        let body = Self::clip_content(content);
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
    }

    async fn touch(
        &self,
        chat_id: &str,
        title: Option<&str>,
        updated_at: &str,
    ) -> Result<(), RepositoryError> {
        let conn = self.conn.lock().map_err(|err| {
            RepositoryError::Database(format!("对话库锁中毒: {err}"))
        })?;
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
    }

    async fn list(&self, limit: u32) -> Result<Vec<AgentChatSummary>, RepositoryError> {
        let conn = self.conn.lock().map_err(|err| {
            RepositoryError::Database(format!("对话库锁中毒: {err}"))
        })?;
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
            let (id, title, created_at, updated_at, message_count) = row.map_err(Self::map_err)?;
            out.push(AgentChatSummary {
                id,
                title,
                created_at: parse_datetime(&created_at),
                updated_at: parse_datetime(&updated_at),
                message_count: message_count as u64,
            });
        }
        Ok(out)
    }

    async fn find(&self, id: &str) -> Result<Option<AgentChatDetail>, RepositoryError> {
        let conn = self.conn.lock().map_err(|err| {
            RepositoryError::Database(format!("对话库锁中毒: {err}"))
        })?;
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
    }

    async fn delete(&self, id: &str) -> Result<bool, RepositoryError> {
        let conn = self.conn.lock().map_err(|err| {
            RepositoryError::Database(format!("对话库锁中毒: {err}"))
        })?;
        let changed = conn
            .execute("DELETE FROM agent_chats WHERE id = ?1", params![id])
            .map_err(Self::map_err)?;
        Ok(changed > 0)
    }

    async fn clear(&self) -> Result<u64, RepositoryError> {
        let conn = self.conn.lock().map_err(|err| {
            RepositoryError::Database(format!("对话库锁中毒: {err}"))
        })?;
        let changed = conn
            .execute("DELETE FROM agent_chats", [])
            .map_err(Self::map_err)?;
        Ok(changed as u64)
    }
}
