//! SqliteSettingsStore 集成测试。
//! 覆盖 FR-SET-006：设置字段必须作为一次原子写入，不允许半更新状态。

use std::sync::{Arc, Mutex};

use app_lib::domain::ports::SettingsStore;
use app_lib::domain::settings::AppSettings;
use app_lib::infrastructure::sqlite::settings_store::SqliteSettingsStore;

fn new_store() -> (SqliteSettingsStore, Arc<Mutex<rusqlite::Connection>>) {
    let mut conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    app_lib::infrastructure::sqlite::migrations::run_migrations(&mut conn)
        .expect("run migrations");
    let conn = Arc::new(Mutex::new(conn));
    (SqliteSettingsStore::new(conn.clone()), conn)
}

#[tokio::test]
async fn load_without_prior_save_returns_defaults() {
    let (store, _conn) = new_store();
    let settings = store.load().await.unwrap();
    let defaults = AppSettings::default();
    assert_eq!(settings.max_history, defaults.max_history);
    assert_eq!(settings.retention_days, defaults.retention_days);
    assert_eq!(settings.capture_enabled, defaults.capture_enabled);
    assert_eq!(settings.shortcut, defaults.shortcut);
}

#[tokio::test]
async fn save_persists_all_fields_together() {
    let (store, conn) = new_store();

    store
        .save(AppSettings {
            max_history: 42,
            retention_days: 7,
            capture_enabled: false,
            shortcut: "Alt+Space".to_string(),
        })
        .await
        .unwrap();

    let conn = conn.lock().unwrap();
    let read_key = |key: &str| -> String {
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(read_key("max_history"), "42");
    assert_eq!(read_key("retention_days"), "7");
    assert_eq!(read_key("capture_enabled"), "false");
    assert_eq!(read_key("shortcut"), "Alt+Space");
}

#[tokio::test]
async fn save_then_load_roundtrips_exactly() {
    let (store, _conn) = new_store();

    let next = AppSettings {
        max_history: 500,
        retention_days: 14,
        capture_enabled: true,
        shortcut: "CmdOrCtrl+Shift+C".to_string(),
    };
    store.save(next.clone()).await.unwrap();

    let loaded = store.load().await.unwrap();
    assert_eq!(loaded.max_history, next.max_history);
    assert_eq!(loaded.retention_days, next.retention_days);
    assert_eq!(loaded.capture_enabled, next.capture_enabled);
    assert_eq!(loaded.shortcut, next.shortcut);
}

#[tokio::test]
async fn repeated_saves_do_not_leave_stale_keys() {
    let (store, _conn) = new_store();

    store
        .save(AppSettings {
            max_history: 100,
            retention_days: 30,
            capture_enabled: true,
            shortcut: "CmdOrCtrl+Shift+V".to_string(),
        })
        .await
        .unwrap();
    store
        .save(AppSettings {
            max_history: 200,
            retention_days: 0,
            capture_enabled: false,
            shortcut: "Alt+H".to_string(),
        })
        .await
        .unwrap();

    let loaded = store.load().await.unwrap();
    assert_eq!(loaded.max_history, 200);
    assert_eq!(loaded.retention_days, 0);
    assert!(!loaded.capture_enabled);
    assert_eq!(loaded.shortcut, "Alt+H");
}

#[tokio::test]
async fn empty_shortcut_roundtrips() {
    let (store, _conn) = new_store();

    store
        .save(AppSettings {
            max_history: 1000,
            retention_days: 30,
            capture_enabled: true,
            shortcut: String::new(),
        })
        .await
        .unwrap();

    let loaded = store.load().await.unwrap();
    assert_eq!(loaded.shortcut, "");
}
