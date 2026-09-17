//! AppKit 相关的三件事：渲染应用真实图标、模拟 ⌘V、把焦点还给上一个应用。
//!
//! 图标渲染由调用方通过 `AppHandle::run_on_main_thread` 调度到主线程执行，
//! 避免 AppKit 在后台线程上出问题。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep,
    NSBitmapImageRepPropertyKey, NSImage, NSRunningApplication, NSWorkspace,
};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGKeyCode,
};
use objc2_foundation::{NSDictionary, NSString};

/// 唤起本应用窗口之前的那个前台应用（bundle identifier）。
///
/// 由 [`record_frontmost_app`] 写入，[`simulate_paste`] 读取。
static PREVIOUS_FRONTMOST: Mutex<Option<String>> = Mutex::new(None);

// ---------------------------------------------------------------------------
//  应用图标
// ---------------------------------------------------------------------------

/// 把某个应用的名字（System Events 报的进程名，如 "idea"）解析成 `.app` 的绝对路径。
fn find_app_bundle_path(app_name: &str) -> Option<String> {
    if let Some(path) = find_running_app_path(app_name) {
        return Some(path);
    }

    // 应用没在运行时只能按目录名找。这里的名字来自进程名，和 `.app` 的目录名
    // 经常对不上（"idea" vs "IntelliJ IDEA.app"），所以先查别名表，再逐个目录
    // 不区分大小写地比对。
    let home = std::env::var("HOME").unwrap_or_default();
    let dirs = [
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from(&home).join("Applications"),
    ];

    let mut candidates = vec![app_name];
    if let Some(alias) = bundle_alias(app_name) {
        candidates.push(alias);
    }

    for candidate in candidates {
        let file_name = format!("{candidate}.app");
        for dir in &dirs {
            let direct = dir.join(&file_name);
            if direct.is_dir() {
                return Some(direct.to_string_lossy().to_string());
            }
            if let Some(found) = find_case_insensitive(dir, &file_name) {
                return Some(found);
            }
        }
    }
    None
}

/// 进程名 → `.app` 目录名的别名表。
///
/// 只收「进程名和目录名差得比较远、靠大小写匹配也找不到」的常见应用；
/// 能靠可执行文件名对上的（WebStorm、PyCharm、GoLand…）不需要在这里登记。
const BUNDLE_ALIASES: &[(&str, &str)] = &[
    ("idea", "IntelliJ IDEA"),
    ("idea ce", "IntelliJ IDEA CE"),
    ("code", "Visual Studio Code"),
    ("chrome", "Google Chrome"),
    ("msedge", "Microsoft Edge"),
    ("wechat", "WeChat"),
];

fn bundle_alias(app_name: &str) -> Option<&'static str> {
    let wanted = app_name.to_lowercase();
    BUNDLE_ALIASES
        .iter()
        .find(|(process_name, _)| *process_name == wanted)
        .map(|(_, bundle_name)| *bundle_name)
}

/// 在目录里不区分大小写地找一个条目。找不到返回 `None`。
fn find_case_insensitive(dir: &Path, file_name: &str) -> Option<String> {
    let wanted = file_name.to_lowercase();
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().to_lowercase() == wanted {
            return Some(entry.path().to_string_lossy().to_string());
        }
    }
    None
}

/// 在正在运行的应用里按名字找。返回 `.app` 的绝对路径。
///
/// 要同时比对**本地化显示名**和**可执行文件名**，因为 System Events 报的是后者：
/// IntelliJ IDEA 的显示名是 "IntelliJ IDEA"，进程名却是 "idea"。
/// 早期的实现只比对显示名且区分大小写，`"IntelliJ IDEA".contains("idea")` 为 false，
/// 于是 IDEA 的图标一直取不到，界面上只能看到一个 emoji 兜底。
fn find_running_app_path(app_name: &str) -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let running = workspace.runningApplications();
    let wanted = app_name.to_lowercase();

    // 精确同名优先；名字只是「包含」关系时先记下来，循环结束再用，
    // 免得 "Chrome" 抢在 "Google Chrome" 前面匹配上。
    let mut fuzzy: Option<String> = None;
    for index in 0..running.count() {
        let app = running.objectAtIndex(index);
        let Some(url) = app.bundleURL() else {
            continue;
        };
        let Some(path) = url.path() else {
            continue;
        };
        let path = path.to_string();

        let localized = app
            .localizedName()
            .map(|name| name.to_string().to_lowercase())
            .unwrap_or_default();
        let executable = app
            .executableURL()
            .and_then(|url| url.path())
            .map(|path| path.to_string())
            .and_then(|path| path.rsplit('/').next().map(str::to_string))
            .map(|name| name.to_lowercase())
            .unwrap_or_default();

        if localized == wanted || executable == wanted {
            return Some(path);
        }
        if fuzzy.is_none() && (localized.contains(&wanted) || executable.contains(&wanted)) {
            fuzzy = Some(path);
        }
    }
    fuzzy
}

/// 渲染应用图标为 PNG 字节。**必须在主线程调用。**
///
/// 返回 `None` 表示系统里找不到这个应用。
///
/// 注意：系统给的图标表示（`NSISIconImageRep`）最小也是几十 px、最大到 2048px，
/// 直接编码出来能到 1~2MB。所以这里出来的是原图，落盘时再用
/// [`save_icon_png`] 缩到列表真正需要的尺寸。
pub fn render_app_icon_png(app_name: &str) -> Option<Vec<u8>> {
    autoreleasepool(|_pool| {
        let app_path = find_app_bundle_path(app_name)?;
        let ns_path = NSString::from_str(&app_path);

        let workspace = NSWorkspace::sharedWorkspace();
        let icon: Retained<NSImage> = workspace.iconForFile(&ns_path);
        let tiff = icon.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;

        let properties: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> =
            NSDictionary::new();
        // SAFETY: properties 传的是空字典，类型与方法签名一致。
        let png = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
        }?;
        Some(png.to_vec())
    })
}

/// 图标在缓存里保留的最大边长（px）。
///
/// 列表里只显示 36pt，2x 屏也就是 72px，128px 足够清晰且留了余量。
const ICON_CACHE_MAX_PX: u32 = 128;

/// 把图标 PNG 写进缓存，并缩到 [`ICON_CACHE_MAX_PX`]。
///
/// 用系统自带的 `sips` 而不是自己在 AppKit 里重绘：
/// 图标表示的类不是 `NSBitmapImageRep`（是 `NSISIconImageRep`），
/// 没法直接挑一张小的编码出来；重绘则要引入 CoreGraphics 绑定 + 在主线程建
/// 图形上下文，为一个列表小图标不值得。`sips` 一次约 60ms，且每个应用只跑一次。
///
/// 缩放失败不影响可用：最坏情况是缓存文件大一点，图还是能正常显示。
pub fn save_icon_png(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;

    let output = Command::new("sips")
        .arg("-Z")
        .arg(ICON_CACHE_MAX_PX.to_string())
        .arg(path)
        .output();

    match output {
        Ok(output) if output.status.success() => {}
        Ok(output) => tracing::warn!(
            path = %path.display(),
            stderr = %String::from_utf8_lossy(&output.stderr).trim(),
            "sips 缩放图标失败，保留原尺寸"
        ),
        Err(err) => tracing::warn!(error = %err, "无法执行 sips，图标保留原尺寸"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
//  焦点归还 + 模拟粘贴
// ---------------------------------------------------------------------------

/// 记住当前前台应用，一键粘贴时要把焦点交还给它。
///
/// 两个调用时机，都只在前台**不是自己**时才写入，所以顺序无关紧要：
/// 1. 全局快捷键唤起窗口**之前** —— 这时前台还是用户原来在用的应用，最准确；
/// 2. 窗口**失焦**时 —— 如果系统在隐藏窗口后已经把焦点交还给了别的应用，顺手记下来。
///
/// 之所以要记：窗口 `hide()` 之后焦点不一定会回到之前的应用。如果本应用还是
/// active，模拟出来的 ⌘V 会发给自己，用户看到的就是「点了一下，什么都没粘贴出来」。
pub fn record_frontmost_app() {
    if is_self_frontmost() {
        return;
    }
    let Some(bundle_id) = frontmost_bundle_id() else {
        return;
    };
    if let Ok(mut guard) = PREVIOUS_FRONTMOST.lock() {
        *guard = Some(bundle_id);
    }
}

fn frontmost_application() -> Option<Retained<NSRunningApplication>> {
    NSWorkspace::sharedWorkspace().frontmostApplication()
}

fn frontmost_bundle_id() -> Option<String> {
    frontmost_application()?
        .bundleIdentifier()
        .map(|id| id.to_string())
}

fn self_bundle_id() -> Option<String> {
    NSRunningApplication::currentApplication()
        .bundleIdentifier()
        .map(|id| id.to_string())
}

fn is_self_frontmost() -> bool {
    matches!(
        (frontmost_bundle_id(), self_bundle_id()),
        (Some(current), Some(mine)) if current == mine
    )
}

/// 把焦点还给唤起窗口之前的应用。返回是否真的发出了激活请求。
fn restore_previous_app() -> bool {
    let previous = PREVIOUS_FRONTMOST
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let Some(previous) = previous else {
        return false;
    };

    let bundle_id = NSString::from_str(&previous);
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle_id);
    if apps.count() == 0 {
        // 之前的应用已经退出了，没什么可激活的。
        return false;
    }
    apps.objectAtIndex(0)
        .activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows)
}

/// 通过 CoreGraphics 直接投递 ⌘V，把剪贴板内容粘贴到当前焦点所在的应用。
///
/// 需要「辅助功能」权限（系统设置 → 隐私与安全性 → 辅助功能）。
///
/// **为什么不用 `osascript ... keystroke`**：那条路要过两道权限 ——
/// 先要「自动化」权限去指挥 System Events，再要「辅助功能」权限让 System Events
/// 肯发按键。被拒时返回的错误码和权限名对不上（实测 macOS 26 返回
/// `1002 errAEEventNotPermitted`，文案还是中文的「osascript 不允许发送按键」），
/// 用户看完根本不知道要去开哪个开关。改成 CGEvent 之后只剩一道「辅助功能」，
/// 而且能在发送**之前**用 [`has_accessibility_permission`] 查出来，
/// 提示就能写得非常明确。
pub fn simulate_paste() -> Result<(), String> {
    // 焦点还在自己身上时，⌘V 会发给自己，什么也不会发生，先把焦点还回去。
    //
    // `activateWithOptions` 只是**请求**系统切换前台应用，是异步的；紧接着就把按键
    // 递出去会打空（事件落在一个正在失去焦点的窗口上）。所以这里要等一下，
    // 让目标应用真正拿到焦点。80ms 是实测能稳定生效的最小量级，
    // 再短会偶发失败，再长会让用户明显感到「点了要顿一下」。
    if is_self_frontmost() && restore_previous_app() {
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    if !has_accessibility_permission() {
        // 顺手把授权页打开：用户已经点了「粘贴」，意图很明确，
        // 与其让他照着提示自己找三层菜单，不如直接送到开关面前。
        open_accessibility_settings();
        return Err(MISSING_ACCESSIBILITY_HINT.to_string());
    }

    post_command_v()
}

/// 打开系统设置的「辅助功能」面板。
///
/// 用 URL scheme 而不是 `AXIsProcessTrustedWithOptions(prompt: true)`：
/// 后者要构造 CFDictionary、处理 CFString 外链常量、还要判定返回值，
/// 代码量翻几倍，效果却差不多（都是把用户送到同一个开关前面）。
fn open_accessibility_settings() {
    let result = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();

    if let Err(err) = result {
        // 打不开也不影响主流程：提示里已经写了手动路径。
        tracing::warn!(error = %err, "无法打开系统设置的辅助功能面板");
    }
}

/// 缺「辅助功能」权限时的提示。
///
/// 单独拎成常量：界面提示与测试要引用同一份文案，不能各写一份。
const MISSING_ACCESSIBILITY_HINT: &str = "已复制到剪贴板，但没有「辅助功能」权限，无法自动粘贴。\
     已为你打开「系统设置 → 隐私与安全性 → 辅助功能」，请打开 ClipMaster Pro 的开关。\
     开关是刚打开的，要重启一次本应用才生效。";

/// 「V」键的虚拟键码（`kVK_ANSI_V`）。
///
/// 虚拟键码对应键盘上的**物理位置**，与输入法和键盘布局无关，
/// 所以这里写死 0x09 在中文、日文、Dvorak 布局下都是同一个键。
const KEY_V: CGKeyCode = 0x09;

/// 投递一对 ⌘V 的按下 / 抬起事件。
fn post_command_v() -> Result<(), String> {
    // 事件源声明「这对按键来自本机当前会话」，让事件走和真实键盘一样的路径。
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState);
    let source_ref = source.as_deref();

    let down = CGEvent::new_keyboard_event(source_ref, KEY_V, true)
        .ok_or_else(|| "无法创建 ⌘V 键盘事件（系统资源不足）".to_string())?;
    let up = CGEvent::new_keyboard_event(source_ref, KEY_V, false)
        .ok_or_else(|| "无法创建 ⌘V 键盘事件（系统资源不足）".to_string())?;

    // 修饰键挂在事件本身上，而不是单独发一个 Command 键：
    // 后者在目标应用看来是「按下了 Command」，偶尔会被当成粘滞键处理。
    CGEvent::set_flags(Some(&down), CGEventFlags::MaskCommand);
    CGEvent::set_flags(Some(&up), CGEventFlags::MaskCommand);

    // HIDEventTap 是系统输入流的最上端，等价于真实键盘；
    // 换成 SessionEventTap 后能送达的范围和兼容性都更差。
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&down));
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&up));

    Ok(())
}

// 查询「辅助功能」权限的入口。
//
// 直接声明外部符号，不额外引入 crate：它在 ApplicationServices 框架里，
// 和 AppKit 一样是系统自带的。返回类型是 C 的 `Boolean`（`unsigned char`），
// 不是 Rust 的 `bool`，所以这里用 `u8` 接。
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

/// 当前进程是否已获得「辅助功能」权限。
pub fn has_accessibility_permission() -> bool {
    // SAFETY: 无参数、无副作用、任何线程可调用。
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_permission_message_names_the_right_switch() {
        assert!(
            MISSING_ACCESSIBILITY_HINT.contains("辅助功能"),
            "提示必须指明是哪个权限，否则用户不知道该开什么: {MISSING_ACCESSIBILITY_HINT}"
        );
        assert!(
            MISSING_ACCESSIBILITY_HINT.contains("重启"),
            "授权后要重启才生效，这句不说用户会以为没修好: {MISSING_ACCESSIBILITY_HINT}"
        );
    }

    #[test]
    fn key_v_is_the_ansi_virtual_keycode() {
        assert_eq!(KEY_V, 9, "kVK_ANSI_V = 0x09");
    }

    #[test]
    fn bundle_alias_maps_process_names_to_real_bundle_names() {
        // IDEA 的进程名是 "idea"、目录名是 "IntelliJ IDEA.app"，
        // 只有走别名表才能在应用没运行时也找到它。
        assert_eq!(bundle_alias("idea"), Some("IntelliJ IDEA"));
        assert_eq!(bundle_alias("IDEA"), Some("IntelliJ IDEA"), "别名匹配不区分大小写");
        assert_eq!(bundle_alias("code"), Some("Visual Studio Code"));
    }

    #[test]
    fn bundle_alias_leaves_unknown_apps_alone() {
        // 表里没有的应用必须原样返回 None，否则会拿错误的路径去渲染图标。
        assert_eq!(bundle_alias("Safari"), None);
        assert_eq!(bundle_alias("企业微信"), None);
        // 不要「包含」就命中：webstorm 有自己的 .app，不需要别名。
        assert_eq!(bundle_alias("webstorm"), None);
    }

    #[test]
    fn find_case_insensitive_finds_app_by_different_casing() {
        // 目录名是 "IntelliJ IDEA.app"，但进程名是小写的 "idea"，
        // 大小写不敏感比对是这两者之间唯一的桥梁。
        let dir = std::env::temp_dir().join(format!("cm-icon-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("IntelliJ IDEA.app")).unwrap();

        let found = find_case_insensitive(&dir, "intellij idea.app");
        assert!(found.is_some(), "大小写不同也应该能找到");
        assert!(found.unwrap().ends_with("IntelliJ IDEA.app"));

        assert_eq!(find_case_insensitive(&dir, "Nowhere.app"), None);

        std::fs::remove_dir_all(&dir).ok();
    }
}
