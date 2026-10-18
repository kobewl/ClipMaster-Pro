//! 设置持久化：存放在 SQLite `app_settings` key-value 表中，
//! 与历史数据共享同一个数据库文件和事务边界。
//!
//! 实现 `domain::ports::SettingsStore` trait，Application 层通过 trait 访问，
//! 不直接依赖 SQLite 细节，保证依赖方向 Application → Domain ← Infrastructure。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::capture_policy::default_excluded_apps;
use crate::domain::error::RepositoryError;
use crate::domain::ports::{AgentConfigStore, AgentProviderConfig, SettingsStore};
use crate::domain::settings::AppSettings;
use crate::infrastructure::sqlite::lock_conn;

const KEY_MAX_HISTORY: &str = "max_history";
const KEY_RETENTION_DAYS: &str = "retention_days";
const KEY_CAPTURE_ENABLED: &str = "capture_enabled";
const KEY_SHORTCUT: &str = "shortcut";
const KEY_THEME: &str = "theme";
/// 「不记录这些应用」名单，值是 JSON 字符串数组。
const KEY_EXCLUDED_APPS: &str = "excluded_apps";
// 模型服务的地址与模型名。**密钥不在这里** —— 它走系统钥匙串
// （见 domain/ports.rs 里两个 trait 的注释）。
const KEY_AI_BASE_URL: &str = "ai_base_url";
const KEY_AI_MODEL: &str = "ai_model";

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
            let conn = lock_conn(&conn);
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
            let shortcut = get_value(&conn, KEY_SHORTCUT)?.unwrap_or(defaults.shortcut.clone());
            let theme = get_value(&conn, KEY_THEME)?
                .filter(|v| matches!(v.as_str(), "system" | "light" | "dark"))
                .unwrap_or(defaults.theme.clone());
            // 没存过 → 默认名单；存坏了也回退默认（隐私上偏安全）；存的是 [] 就保持空。
            let excluded_apps = match get_value(&conn, KEY_EXCLUDED_APPS)? {
                None => default_excluded_apps(),
                Some(raw) => serde_json::from_str::<Vec<String>>(&raw).unwrap_or_else(|err| {
                    tracing::warn!(error = %err, "排除应用名单解析失败，回退到默认名单");
                    default_excluded_apps()
                }),
            };

            Ok(AppSettings {
                max_history,
                retention_days,
                capture_enabled,
                shortcut,
                theme,
                excluded_apps,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn save(&self, settings: AppSettings) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = lock_conn(&conn);
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
            set_value(&tx, KEY_THEME, &settings.theme)?;
            let excluded_apps = serde_json::to_string(&settings.excluded_apps)
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            set_value(&tx, KEY_EXCLUDED_APPS, &excluded_apps)?;

            tx.commit()
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}

/// 两个键都在才算配置过；只有一半视为没配，避免拼出地址与模型对不上的组合。
#[async_trait]
impl AgentConfigStore for SqliteSettingsStore {
    async fn load_agent_config(&self) -> Result<Option<AgentProviderConfig>, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            let base_url = get_value(&conn, KEY_AI_BASE_URL)?;
            let model = get_value(&conn, KEY_AI_MODEL)?;
            match (base_url, model) {
                (Some(base_url), Some(model))
                    if !base_url.trim().is_empty() && !model.trim().is_empty() =>
                {
                    Ok(Some(AgentProviderConfig { base_url, model }))
                }
                _ => Ok(None),
            }
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn save_agent_config(&self, config: AgentProviderConfig) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = lock_conn(&conn);
            let tx = conn
                .transaction()
                .map_err(|e| RepositoryError::Database(e.to_string()))?;
            set_value(&tx, KEY_AI_BASE_URL, &config.base_url)?;
            set_value(&tx, KEY_AI_MODEL, &config.model)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::capture_policy::DEFAULT_EXCLUDED_APPS;

    fn store() -> SqliteSettingsStore {
        let mut conn = Connection::open_in_memory().expect("内存库");
        crate::infrastructure::sqlite::migrations::run_migrations(&mut conn).expect("跑迁移");
        SqliteSettingsStore::new(Arc::new(Mutex::new(conn)))
    }

    #[tokio::test]
    async fn missing_excluded_apps_key_loads_defaults() {
        let loaded = store().load().await.expect("读取设置");
        assert_eq!(loaded.excluded_apps, default_excluded_apps());
        assert_eq!(loaded.excluded_apps.len(), DEFAULT_EXCLUDED_APPS.len());
    }

    #[tokio::test]
    async fn empty_excluded_apps_round_trip_stays_empty() {
        let store = store();
        let settings = AppSettings {
            excluded_apps: Vec::new(),
            ..Default::default()
        };
        store.save(settings).await.expect("保存设置");
        let loaded = store.load().await.expect("读取设置");
        assert!(
            loaded.excluded_apps.is_empty(),
            "用户清空的名单不回填默认值"
        );
    }

    #[tokio::test]
    async fn custom_excluded_apps_round_trip() {
        let store = store();
        let apps = vec!["com.example.vault".to_string(), "钥匙串访问".to_string()];
        let settings = AppSettings {
            excluded_apps: apps.clone(),
            ..Default::default()
        };
        store.save(settings).await.expect("保存设置");
        assert_eq!(store.load().await.expect("读取设置").excluded_apps, apps);
    }

    #[tokio::test]
    async fn corrupt_excluded_apps_falls_back_to_defaults() {
        let store = store();
        {
            let conn = store.conn.lock().unwrap();
            set_value(&conn, KEY_EXCLUDED_APPS, "not json").unwrap();
        }
        let loaded = store.load().await.expect("读取设置");
        assert_eq!(loaded.excluded_apps, default_excluded_apps());
    }
}
