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
use crate::commands::system_commands::{check_for_updates, get_autostart_enabled, set_autostart_enabled};
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
        // 应用内自动安装更新（download_and_install）将来才接；现在「检查更新」
        // 走 GitHub API（system_commands.rs），不经过这个插件。注册保留 +
        // tauri.conf.json 里的空占位是插件初始化的硬性要求（缺了启动即 panic）。
        .plugin(tauri_plugin_updater::Builder::new().build())
        // 前端「前往下载」按钮的 openUrl 靠它；不注册的话调用直接报
        // "plugin opener not found"。
        .plugin(tauri_plugin_opener::init())
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

            // 菜单栏托盘：关窗驻留后的常驻入口（显示/隐藏、暂停/恢复采集、
            // 打开设置、退出）。创建失败不致命 —— 快捷键仍然可用，只是少一个入口。
            if let Err(err) = crate::lifecycle::tray::setup_tray(handle, settings.capture_enabled) {
                tracing::warn!(error = %err, "创建菜单栏托盘失败");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 窗口失焦时，如果前台已经换成了别的应用（比如隐藏窗口后系统把焦点交还了），
            // 顺势把它记下来，一键粘贴时才知道该把 ⌘V 发给谁。
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::Focused(false) = event {
                crate::infrastructure::appicon::macos::record_frontmost_app();
            }
            if let tauri::WindowEvent::Destroyed = event {}

            // 点红色关闭按钮 / ⌘W = 隐藏到后台，**不退出进程**。
            //
            // 这是采集类应用的生命线：Tauri 默认「最后一个窗口关闭 = 进程退出」
            // （tauri 2.x 里窗口关闭触发的 ExitRequested code=None 默认放行），
            // 用户顺手点一下 ✕，采集就静默停了，只会觉得「应用莫名其妙不工作了」。
            // 常驻后台是本应用的常态，真正的退出走 ⌘Q / Dock 右键「退出」。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
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
            check_for_updates,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // macOS：点 Dock 图标重新唤起主窗口。
            //
            // 关窗隐藏（见 on_window_event）之后，进程还活着但没有可见窗口，
            // 系统的默认反应只是把应用带到前台——窗口还是藏着的，看起来像「点了没反应」。
            // 这里把 Dock 点击统一当作「唤起」：不管窗口藏没藏，show + 聚焦。
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(runtime) = app_handle.try_state::<AppRuntime>() {
                    runtime.shutdown();
                }
            }
        });
}
