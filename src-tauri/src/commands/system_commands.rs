//! 系统集成类命令：开机自启。
//!
//! **自启状态不存进 settings 表。** 真正的开关是系统里的那个登录项文件
//! （macOS 上是 `~/Library/LaunchAgents/*.plist`），数据库里再存一份就有两个
//! 真相来源：用户在系统设置里改掉、或者系统清理了 plist，两边就对不上了。
//! 所以这里每次都直接问插件，界面上显示的一定是实际生效的状态。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::domain::error::CommandError;
use crate::lifecycle::autostart::is_autostart_locked;

#[tauri::command]
pub async fn get_autostart_enabled(app: AppHandle) -> Result<bool, CommandError> {
    app.autolaunch()
        .is_enabled()
        .map_err(|err| CommandError::system(format!("读取开机自启状态失败: {err}")))
}

#[tauri::command]
pub async fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), CommandError> {
    if is_autostart_locked() {
        return Err(CommandError::system(
            "开发版不能设置开机自启：它需要开发服务器才能显示界面，注册成登录项后\
             开机只会弹出一个空白窗口。装好正式版本再来开这个开关。"
                .to_string(),
        ));
    }

    let manager = app.autolaunch();
    let result = if enabled { manager.enable() } else { manager.disable() };

    // 失败时把「想干什么」写进 message：只报一句系统错误，用户不知道是哪个开关没设上。
    result.map_err(|err| {
        let action = if enabled { "开启" } else { "关闭" };
        CommandError::system(format!("{action}开机自启失败: {err}"))
    })
}
