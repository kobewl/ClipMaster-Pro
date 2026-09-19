//! macOS 菜单栏托盘：常驻后台时唯一的可见入口。
//!
//! 「关窗即驻留」（见 lib.rs 的 CloseRequested 拦截）之后，托盘回答两个问题：
//! 「怎么找回来」和「怎么暂停」。四个菜单项构成最小闭环：
//! 显示/隐藏主窗口 · 暂停/恢复采集 · 打开设置 · 退出应用。
//!
//! 状态一致性（避免双状态源）：
//! - 采集开关：唯一真相源是 settings 表。托盘的切换与设置面板、状态栏点击
//!   汇入同一条写入路径 [`crate::lifecycle::runtime::AppRuntime::set_capture_enabled`]，
//!   托盘菜单文案只是真相源的一面镜子（由该路径统一刷新）；
//! - 窗口显隐：完全复用全局快捷键的 `toggle_main_window`，不另写一份；
//! - 退出：`AppHandle::exit(0)` 走 `RunEvent::ExitRequested`，与「关窗隐藏」
//!   明确区分（关窗 = 驻留，托盘退出 = 真退出，清理逻辑在 lib.rs 统一执行）。
//!
//! 图标：`docs/design/logo/clipmaster-logo-mono.svg` 是设计系统钦点的
//! 「系统菜单栏模板图」来源（一块剪贴板剪影）。按其几何栅格化为 44×44
//! 纯黑 PNG（见 `icons/tray-icon.png` 的生成方式与
//! `tray_icon_asset_is_valid_template` 测试），以 macOS template image
//! 方式加载 —— 系统自动适配明暗两种菜单栏。

use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::domain::error::AppError;
use crate::lifecycle::runtime::AppRuntime;
use crate::lifecycle::shortcut::toggle_main_window;

/// 嵌入的托盘图标（44×44 纯黑模板图）。
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../icons/tray-icon.png");

pub const MENU_ID_SHOW_HIDE: &str = "cm_show_hide";
pub const MENU_ID_TOGGLE_CAPTURE: &str = "cm_toggle_capture";
pub const MENU_ID_OPEN_SETTINGS: &str = "cm_open_settings";
pub const MENU_ID_QUIT: &str = "cm_quit";

/// 采集菜单项的文案：开启中 → 提示可暂停；已暂停 → 提示可恢复。
fn capture_menu_label(capture_enabled: bool) -> &'static str {
    if capture_enabled {
        "暂停采集"
    } else {
        "恢复采集"
    }
}

/// 需要在运行时改文案的菜单项句柄。托盘创建后交给 Tauri 管理。
struct TrayMenuHandles {
    toggle_capture: MenuItem<Wry>,
}

/// 采集开关变化后刷新托盘菜单文案。真相源在 settings 表，
/// 这里只是把镜像刷对 —— 没有托盘（创建失败等）时静默跳过。
pub fn update_capture_menu_item(app: &AppHandle, capture_enabled: bool) {
    if let Some(handles) = app.try_state::<TrayMenuHandles>() {
        if let Err(err) = handles
            .toggle_capture
            .set_text(capture_menu_label(capture_enabled))
        {
            tracing::warn!(error = %err, "更新托盘采集菜单文案失败");
        }
    }
}

/// 创建菜单栏托盘。`capture_enabled` 是创建时刻的采集状态（菜单初始文案）。
///
/// 失败不致命：调用方记 warn 继续 —— 托盘只是入口之一，快捷键仍可用。
pub fn setup_tray(app: &AppHandle, capture_enabled: bool) -> tauri::Result<()> {
    let show_hide =
        MenuItem::with_id(app, MENU_ID_SHOW_HIDE, "显示 / 隐藏主窗口", true, None::<&str>)?;
    let toggle_capture = MenuItem::with_id(
        app,
        MENU_ID_TOGGLE_CAPTURE,
        capture_menu_label(capture_enabled),
        true,
        None::<&str>,
    )?;
    let open_settings =
        MenuItem::with_id(app, MENU_ID_OPEN_SETTINGS, "打开设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_ID_QUIT, "退出 ClipMaster Pro", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&show_hide)
        .item(&toggle_capture)
        .separator()
        .item(&open_settings)
        .separator()
        .item(&quit)
        .build()?;

    let icon = tauri::image::Image::from_bytes(TRAY_ICON_PNG)?;

    TrayIconBuilder::with_id("cm-tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("ClipMaster Pro")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_menu_event)
        .build(app)?;

    // 菜单项句柄交给状态树，运行时改文案用（见 update_capture_menu_item）。
    // MenuItem 是原生菜单项的引用句柄，move 进结构体不影响已挂到菜单里的项。
    app.manage(TrayMenuHandles { toggle_capture });
    Ok(())
}

fn handle_menu_event(app: &AppHandle<Wry>, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        MENU_ID_SHOW_HIDE => toggle_main_window(app),
        MENU_ID_TOGGLE_CAPTURE => {
            // 菜单事件回调是同步的；采集开关要读写数据库，丢到异步运行时。
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = toggle_capture(&app).await {
                    tracing::warn!(error = %err, "托盘切换采集状态失败");
                }
            });
        }
        MENU_ID_OPEN_SETTINGS => {
            // 设置面板是前端的本地状态：先把窗口拉起来聚焦，再通知前端开面板。
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            if let Err(err) = app.emit("ui://open-settings", ()) {
                tracing::warn!(error = %err, "发送 ui://open-settings 事件失败");
            }
        }
        MENU_ID_QUIT => {
            // 与「关窗隐藏」的本质区别：这里真正结束进程。
            // ExitRequested（code=Some）会触发 lib.rs 里的 runtime.shutdown()。
            app.exit(0);
        }
        _ => {}
    }
}

async fn toggle_capture(app: &AppHandle<Wry>) -> Result<(), AppError> {
    let Some(runtime) = app.try_state::<AppRuntime>() else {
        return Ok(());
    };
    let current = runtime.settings.get().await?.capture_enabled;
    runtime.set_capture_enabled(app, !current).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_menu_label_reflects_state() {
        assert_eq!(capture_menu_label(true), "暂停采集", "开启中 → 提示可暂停");
        assert_eq!(capture_menu_label(false), "恢复采集", "已暂停 → 提示可恢复");
    }

    /// 托盘图标资产本身就要满足 macOS 模板图规范：
    /// 纯黑 + 透明背景 + 44×44（2x 尺寸）。生成脚本改了形状，这个测试就该红。
    #[test]
    fn tray_icon_asset_is_valid_template() {
        let image = tauri::image::Image::from_bytes(TRAY_ICON_PNG).expect("PNG 解码失败");
        assert_eq!(image.width(), 44);
        assert_eq!(image.height(), 44);

        let rgba = image.rgba();
        let (mut opaque, mut transparent) = (0usize, 0usize);
        for px in rgba.chunks_exact(4) {
            let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
            if a == 0 {
                transparent += 1;
            } else {
                opaque += 1;
                assert!(
                    r == 0 && g == 0 && b == 0,
                    "模板图必须纯黑（由系统自行反色），出现 RGB({r},{g},{b})"
                );
            }
        }
        assert!(opaque > 0, "剪影必须有可见内容");
        assert!(transparent > 0, "剪影必须有透明背景");
    }

    #[test]
    fn menu_ids_are_stable_and_distinct() {
        // 菜单 id 是事件分发的路由键，一旦变更用户手里跑着的旧事件仍会按 id 匹配，
        // 保持它们稳定且互不相同是隐含契约。
        let ids = [
            MENU_ID_SHOW_HIDE,
            MENU_ID_TOGGLE_CAPTURE,
            MENU_ID_OPEN_SETTINGS,
            MENU_ID_QUIT,
        ];
        for (i, a) in ids.iter().enumerate() {
            for b in ids.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
            assert!(a.starts_with("cm_"));
        }
    }
}
