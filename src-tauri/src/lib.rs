pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod lifecycle;

use tauri::Manager;

use crate::commands::clipboard_commands::{
    clear_history, copy_clipboard_item, copy_text_to_clipboard, create_group, delete_clipboard_item,
    delete_group, get_settings, get_source_icons, list_clipboard_items, list_groups,
    paste_clipboard_item, paste_text, set_capture_enabled, set_item_group, update_group,
    update_settings, update_shortcut,
};
use crate::commands::system_commands::{get_autostart_enabled, set_autostart_enabled};
use crate::lifecycle::autostart::{launched_by_autostart, AUTOSTART_FLAG};
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
        // 注册登录项时带上 --autostart，启动时据此决定要不要藏窗口。
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .setup(|app| {
            let handle = app.handle();

            // 开机自启拉起来的不弹窗口：用户多半还在等系统启动完成，
            // 这个应用只是常驻后台等全局快捷键，弹窗会打断他。
            if launched_by_autostart(std::env::args()) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

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
            // 窗口失焦时，如果前台已经换成了别的应用（比如隐藏窗口后系统把焦点交还了），
            // 顺势把它记下来，一键粘贴时才知道该把 ⌘V 发给谁。
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::Focused(false) = event {
                crate::infrastructure::appicon::macos::record_frontmost_app();
            }
            if let tauri::WindowEvent::Destroyed = event {}
        })
        .invoke_handler(tauri::generate_handler![
            list_clipboard_items,
            delete_clipboard_item,
            set_item_group,
            copy_clipboard_item,
            paste_clipboard_item,
            copy_text_to_clipboard,
            paste_text,
            clear_history,
            list_groups,
            create_group,
            update_group,
            delete_group,
            get_settings,
            update_settings,
            set_capture_enabled,
            update_shortcut,
            get_source_icons,
            get_autostart_enabled,
            set_autostart_enabled,
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
