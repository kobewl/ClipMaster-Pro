pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod lifecycle;

use tauri::Manager;

use crate::commands::clipboard_commands::{
    clear_history, copy_clipboard_item, create_group, delete_clipboard_item, delete_group,
    get_settings, list_clipboard_items, list_groups, paste_clipboard_item, set_capture_enabled,
    set_item_group, update_group, update_settings, update_shortcut,
};
use crate::lifecycle::runtime::{build_runtime, AppRuntime};
use crate::lifecycle::shortcut::register_global_shortcut;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle();
            let runtime = build_runtime(handle).map_err(|e| {
                tracing::error!(error = %e, "初始化 AppRuntime 失败");
                e
            })?;

            let settings = {
                let store = runtime.settings_store();
                tauri::async_runtime::block_on(store.load()).unwrap_or_default()
            };
            if let Err(err) = register_global_shortcut(handle, &settings.shortcut) {
                tracing::warn!(error = %err, shortcut = %settings.shortcut, "启动时注册全局快捷键失败");
            }

            app.manage(runtime);
            Ok(())
        })
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {}
        })
        .invoke_handler(tauri::generate_handler![
            list_clipboard_items,
            delete_clipboard_item,
            set_item_group,
            copy_clipboard_item,
            paste_clipboard_item,
            clear_history,
            list_groups,
            create_group,
            update_group,
            delete_group,
            get_settings,
            update_settings,
            set_capture_enabled,
            update_shortcut,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(runtime) = app_handle.try_state::<AppRuntime>() {
                    runtime.shutdown();
                }
            }
        });
}
