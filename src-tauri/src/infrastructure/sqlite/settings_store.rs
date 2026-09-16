//! 设置持久化：存放在 SQLite `app_settings` key-value 表中，
//! 与历史数据共享同一个数据库文件和事务边界。
//!
//! 实现 `domain::ports::SettingsStore` trait，Application 层通过 trait 访问，
//! 不直接依赖 SQLite 细节，保证依赖方向 Application → Domain ← Infrastructure。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::RepositoryError;
use crate::domain::ports::SettingsStore;
use crate::domain::settings::AppSettings;

const KEY_MAX_HISTORY: &str = "max_history";
const KEY_RETENTION_DAYS: &str = "retention_days";
const KEY_CAPTURE_ENABLED: &str = "capture_enabled";
const KEY_SHORTCUT: &str = "shortcut";

pub struct SqliteSettingsStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSettingsStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl SettingsStore for SqliteSettingsStore {
    async fn load(&self) -> Result<AppSettings, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let defaults = AppSettings::default();

            let max_history = get_value(&conn, KEY_MAX_HISTORY)?
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(defaults.max_history);
            let retention_days = get_value(&conn, KEY_RETENTION_DAYS)?
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(defaults.retention_days);
            let capture_enabled = get_value(&conn, KEY_CAPTURE_ENABLED)?
                .map(|v| v == "true")
                .unwrap_or(defaults.capture_enabled);
            let shortcut = get_value(&conn, KEY_SHORTCUT)?
                .unwrap_or(defaults.shortcut.clone());

            Ok(AppSettings {
                max_history,
                retention_days,
                capture_enabled,
                shortcut,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn save(&self, settings: AppSettings) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn
                .transaction()
                .map_err(|e| RepositoryError::Database(e.to_string()))?;

            set_value(&tx, KEY_MAX_HISTORY, &settings.max_history.to_string())?;
            set_value(
                &tx,
                KEY_RETENTION_DAYS,
                &settings.retention_days.to_string(),
            )?;
            set_value(
                &tx,
                KEY_CAPTURE_ENABLED,
                if settings.capture_enabled {
                    "true"
                } else {
                    "false"
                },
            )?;
            set_value(&tx, KEY_SHORTCUT, &settings.shortcut)?;

            tx.commit()
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}

fn get_value(conn: &Connection, key: &str) -> Result<Option<String>, RepositoryError> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| RepositoryError::Database(e.to_string()))
}

fn set_value(conn: &Connection, key: &str, value: &str) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(())
}
