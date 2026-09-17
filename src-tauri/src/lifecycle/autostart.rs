//! 开机自启的启动参数约定。
//!
//! 开机自启把应用拉起来时，用户多半还在等系统启动完成，**不应该弹窗口**：
//! 它只是个常驻后台、等全局快捷键唤起的工具。所以注册登录项时带上一个标记参数，
//! 启动时看到这个参数就把窗口藏起来。

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
