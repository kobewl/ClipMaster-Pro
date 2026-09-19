//! SQLite 实现的 GroupRepository。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::domain::error::RepositoryError;
use crate::domain::model::ClipGroup;
use crate::domain::ports::GroupRepository;

pub struct SqliteGroupRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteGroupRepository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn map_db_err(err: rusqlite::Error) -> RepositoryError {
        RepositoryError::Database(err.to_string())
    }

    fn row_to_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipGroup> {
        let created_at_str: String = row.get("created_at")?;
        let updated_at_str: String = row.get("updated_at")?;
        let item_count: i64 = row.get("item_count")?;
        Ok(ClipGroup {
            id: row.get("id")?,
            name: row.get("name")?,
            color: row.get("color")?,
            sort_order: row.get("sort_order")?,
            item_count: item_count as u64,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

#[async_trait]
impl GroupRepository for SqliteGroupRepository {
    async fn list_groups(&self) -> Result<Vec<ClipGroup>, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut stmt = conn
                .prepare(
                    "SELECT g.id, g.name, g.color, g.sort_order, g.created_at, g.updated_at,
                            COUNT(ci.id) AS item_count
                     FROM clip_groups g
                     LEFT JOIN clipboard_items ci ON ci.group_id = g.id
                     GROUP BY g.id
                     ORDER BY g.sort_order, g.created_at",
                )
                .map_err(Self::map_db_err)?;
            let rows = stmt
                .query_map([], Self::row_to_group)
                .map_err(Self::map_db_err)?;
            let mut groups = Vec::new();
            for row in rows {
                groups.push(row.map_err(Self::map_db_err)?);
            }
            Ok(groups)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn create_group(
        &self,
        name: String,
        color: String,
    ) -> Result<ClipGroup, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();

            let sort_order: i32 = conn
                .query_row(
                    "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM clip_groups",
                    [],
                    |row| row.get(0),
                )
                .map_err(Self::map_db_err)?;

            conn.execute(
                "INSERT INTO clip_groups (id, name, color, sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, name, color, sort_order, now],
            )
            .map_err(Self::map_db_err)?;

            Ok(ClipGroup {
                id,
                name,
                color,
                sort_order,
                item_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn update_group(
        &self,
        id: String,
        name: String,
        color: String,
    ) -> Result<ClipGroup, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let now = Utc::now().to_rfc3339();
            let affected = conn
                .execute(
                    "UPDATE clip_groups SET name = ?1, color = ?2, updated_at = ?3 WHERE id = ?4",
                    params![name, color, now, id],
                )
                .map_err(Self::map_db_err)?;
            if affected == 0 {
                return Err(RepositoryError::NotFound(id));
            }
            conn.query_row(
                "SELECT g.id, g.name, g.color, g.sort_order, g.created_at, g.updated_at,
                        COUNT(ci.id) AS item_count
                 FROM clip_groups g
                 LEFT JOIN clipboard_items ci ON ci.group_id = g.id
                 WHERE g.id = ?1
                 GROUP BY g.id",
                params![&id],
                Self::row_to_group,
            )
            .map_err(|_| {
                // Fallback: return a minimal group
                RepositoryError::Database("query after update failed".to_string())
            })
            .or_else(|_| {
                Ok(ClipGroup {
                    id: id.clone(),
                    name,
                    color,
                    sort_order: 0,
                    item_count: 0,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn delete_group(&self, id: String) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(Self::map_db_err)?;
            tx.execute(
                "UPDATE clipboard_items SET group_id = NULL WHERE group_id = ?1",
                params![id],
            )
            .map_err(Self::map_db_err)?;
            tx.execute("DELETE FROM clip_groups WHERE id = ?1", params![id])
                .map_err(Self::map_db_err)?;
            tx.commit().map_err(Self::map_db_err)?;
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}
