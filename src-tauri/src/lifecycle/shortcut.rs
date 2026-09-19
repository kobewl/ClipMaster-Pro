//! 全局快捷键注册与切换。
//!
//! 对应文档：
//! - FR-SYS-002：提供全局快捷键显示/隐藏主窗口。
//! - FR-SYS-003：快捷键冲突时给出明确反馈，不得静默失败。
//! - FR-SET-003：设置全局快捷键。
//! - macOS 平台设计文档第 5 节：新快捷键注册成功后才注销旧快捷键。

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// 注册全局快捷键，触发时切换主窗口显示/隐藏。
/// 空字符串表示不注册（用户可选择关闭快捷键）。
pub fn register_global_shortcut(app_handle: &AppHandle, shortcut: &str) -> Result<(), String> {
    if shortcut.is_empty() {
        return Ok(());
    }

    let handle = app_handle.clone();
    app_handle
        .global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                toggle_main_window(&handle);
            }
        })
        .map_err(|e| format!("注册快捷键 \"{shortcut}\" 失败: {e}"))
}

/// 切换快捷键：先尝试注册新的，成功后才注销旧的（macOS 平台设计文档第 5 节）。
/// 这样即使新快捷键注册失败（冲突等），旧快捷键仍然可用。
pub fn swap_shortcut(
    app_handle: &AppHandle,
    old_shortcut: &str,
    new_shortcut: &str,
) -> Result<(), String> {
    if old_shortcut == new_shortcut {
        return Ok(());
    }

    // 先注册新的
    if !new_shortcut.is_empty() {
        register_global_shortcut(app_handle, new_shortcut)?;
    }

    // 新的注册成功后才注销旧的
    if !old_shortcut.is_empty() {
        if let Err(err) = app_handle
            .global_shortcut()
            .unregister(old_shortcut)
        {
            tracing::warn!(error = %err, shortcut = old_shortcut, "注销旧快捷键失败（非致命）");
        }
    }

    Ok(())
}

/// 显隐主窗口的统一动作：全局快捷键与托盘「显示 / 隐藏主窗口」共用。
pub(crate) fn toggle_main_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            // 趁前台还是用户原来在用的应用，把它记下来：一键粘贴时要把焦点还给它，
            // 否则模拟出来的 ⌘V 会发回自己（窗口 `hide()` 不会让应用主动让出焦点）。
            #[cfg(target_os = "macos")]
            crate::infrastructure::appicon::macos::record_frontmost_app();

            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}
