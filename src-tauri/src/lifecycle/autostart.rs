//! 开机自启的启动参数约定，以及「别让系统再恢复我一遍」。
//!
//! 开机自启把应用拉起来时，用户多半还在等系统启动完成，**不应该弹窗口**：
//! 它只是个常驻后台、等全局快捷键唤起的工具。所以注册登录项时带上一个标记参数，
//! 启动时看到这个参数就把窗口藏起来。
//!
//! 另外，macOS 自己还有一套「重启后恢复窗口」机制（TAL），会独立于登录项
//! 把应用拉起来 —— 见 [`disable_relaunch_on_login`]。

/// 由开机自启拉起时附带的命令行参数。
pub const AUTOSTART_FLAG: &str = "--autostart";

/// 这次启动是不是开机自启拉起来的。
///
/// 单独抽成函数是为了能测：命令行参数只有启动那一刻拿得到，
/// 藏在 `setup` 闭包里就没法写测试了。
pub fn launched_by_autostart<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == AUTOSTART_FLAG)
}

/// 告诉 macOS：登录时**不要**用「重新打开窗口」把本应用恢复回来。
///
/// 本应用已经有登录项自启这条路径（用户可在设置里开关），系统恢复是第二条
/// 独立路径 —— 两条都生效就会拉起两个实例（实测过：系统恢复那个先到、登录项
/// 那个 90 秒后到）。留一条就够了，登录项那条更可控。
///
/// 用 AppKit 的 `-[NSApplication disableRelaunchOnLogin]`。Apple 对这个方法的
/// 说明正好就是我们的场景：
///
/// > 如果应用因为通过其它机制启动（例如 launchd）而不应被重新启动，
/// > 推荐调用一次 `disableRelaunchOnLogin`，并且**永远不要**配对调用 enable。
///
/// 唯一约束：必须主线程调用（`NSApplication` 的 `MainThreadOnly` 约束），
/// 所以调用点在 `RunEvent::Ready`。
#[cfg(target_os = "macos")]
pub fn disable_relaunch_on_login() {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;

    let Some(mtm) = MainThreadMarker::new() else {
        // 只可能是调用点写错了（放到了非主线程）。退化成「多一个实例」而不是
        // 进程崩溃 —— 采集、快捷键都不受影响，只是又回到系统恢复那条老路。
        tracing::warn!("disableRelaunchOnLogin 必须在主线程调用，已跳过");
        return;
    };

    NSApplication::sharedApplication(mtm).disableRelaunchOnLogin();
}

/// 当前构建是否禁止改动开机自启设置。
///
/// 开发版（`cargo build` / `tauri dev`）的二进制**要靠 Vite 开发服务器才能显示界面**，
/// 把它注册成登录项，开机后只会弹出一个连不上 devUrl 的空白窗口 —— 而且用户
/// 根本不知道是哪里来的。所以在开发版里只允许查询状态，不允许修改。
pub fn is_autostart_locked() -> bool {
    cfg!(debug_assertions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_autostart_flag() {
        assert!(launched_by_autostart(["--autostart"]));
        assert!(launched_by_autostart(["foo", "--autostart", "bar"]));
        assert!(launched_by_autostart(vec!["--autostart".to_string()]));
    }

    #[test]
    fn does_not_match_other_launches() {
        // 正常双击启动、`tauri dev` 启动都不带这个参数
        assert!(!launched_by_autostart::<[&str; 0], &str>([]));
        assert!(!launched_by_autostart(["--autostart-now"]));
        assert!(!launched_by_autostart(["--AutoStart"]));
    }
}
