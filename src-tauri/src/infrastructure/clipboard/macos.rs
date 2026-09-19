//! macOS 剪贴板采集与写入实现。
//!
//! 支持 Text + Image 两种类型。
//! 防自拷贝循环：writer 写入前记录 fingerprint，采集侧匹配后跳过。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContent, ClipboardContext, ClipboardHandler,
    ClipboardWatcher, ClipboardWatcherContext, RustImageData,
};
use tokio::sync::mpsc::Sender;

use crate::domain::error::ClipboardSourceError;
use crate::domain::model::ContentType;
use crate::domain::normalize::{compute_fingerprint, compute_fingerprint_bytes, strip_html_tags};
use crate::domain::ports::{ClipboardEvent, ClipboardSource, ClipboardWriter};
use crate::infrastructure::appicon::macos::frontmost_app_display_name;

/// 待消费的 self-write 指纹队列（先进先出，上限 8 个）。
///
/// 用队列而不是单值：一次「写 HTML」会同时放 HTML 和纯文本两份内容进剪贴板，
/// 采集侧先读到哪份不确定，两份指纹都得能对上。
pub type SelfWriteGuard = Arc<Mutex<Vec<String>>>;

const SELF_WRITE_QUEUE_CAP: usize = 8;

pub fn new_self_write_guard() -> SelfWriteGuard {
    Arc::new(Mutex::new(Vec::new()))
}

/// 记下「接下来这次剪贴板变化是我们自己写的」。
/// 超过上限时丢掉最旧的 —— 正常使用远达不到这个量级，防御性兜底而已。
fn mark_self_write(guard: &SelfWriteGuard, fingerprints: &[String]) {
    let mut guard = guard.lock().expect("mutex poisoned");
    for fingerprint in fingerprints {
        guard.push(fingerprint.clone());
    }
    let overflow = guard.len().saturating_sub(SELF_WRITE_QUEUE_CAP);
    guard.drain(..overflow);
}

/// 采集到的内容指纹若在队列里，消费掉并返回 true（说明是自己写回的，跳过）。
fn matches_and_consume_self_write(guard: &SelfWriteGuard, fingerprint: &str) -> bool {
    let mut guard = guard.lock().expect("mutex poisoned");
    if let Some(pos) = guard.iter().position(|fp| fp == fingerprint) {
        guard.remove(pos);
        true
    } else {
        false
    }
}

struct EventHandler {
    sender: Sender<ClipboardEvent>,
    self_write_guard: SelfWriteGuard,
    image_dir: PathBuf,
}

impl ClipboardHandler for EventHandler {
    fn on_clipboard_change(&mut self) {
        let ctx = match ClipboardContext::new() {
            Ok(ctx) => ctx,
            Err(err) => {
                tracing::warn!(error = %err, "打开 macOS 剪贴板上下文失败");
                return;
            }
        };

        // 采集来源应用信息（在处理剪贴板内容前获取，此时前台应用即复制源）
        let source_info = get_source_info();

        // 优先文件引用：Finder 里 ⌘C 文件时，剪贴板上会同时出现「文件 URL」和
        // 一段路径样子的文本。先查文件，才能把文件拷贝和普通文本区分开。
        if let Ok(files) = ctx.get_files() {
            if !files.is_empty() {
                self.handle_files(files, source_info);
                return;
            }
        }

        // 其次纯文本。注意浏览器等应用复制富文本时会同时带 text/plain 和 text/html，
        // 这里故意让纯文本赢：只有 HTML、没有纯文本的情况才单独收 HTML 条目。
        if let Ok(text) = ctx.get_text() {
            if !text.is_empty() {
                self.handle_text(text, source_info);
                return;
            }
        }

        // 纯 HTML（没有纯文本兜底）：某些网页编辑器只往剪贴板放 text/html。
        if let Ok(html) = ctx.get_html() {
            if !html.is_empty() {
                self.handle_html(html, source_info);
                return;
            }
        }

        // 最后尝试图片
        self.handle_image(&ctx, source_info);
    }
}

struct SourceInfo {
    app_name: Option<String>,
    source_url: Option<String>,
}

fn get_source_info() -> SourceInfo {
    // 前台应用名走 NSWorkspace 进程内查询：零子进程、零「自动化」权限依赖。
    // 旧 osascript 路径（System Events）实测每次复制要付 40ms+ 的子进程底价，
    // 且授权被拒后来源永远为空。失败时返回 None，按「无来源」记录。
    let app_name = frontmost_app_display_name();
    // 只有前台是受支持的浏览器时才会真正 spawn osascript 去查标签页 URL ——
    // 每个浏览器的 AppleScript 字典没有进程内替代，这条子进程保留。
    let source_url = app_name.as_deref().and_then(get_browser_url);
    SourceInfo { app_name, source_url }
}

/// 各浏览器取「当前标签页 URL」的 AppleScript；None = 不是受支持的浏览器。
///
/// 抽成纯函数是为了能单测映射关系（哪些应用名会触发子进程、哪些不会）。
fn browser_script_for(app_name: &str) -> Option<&'static str> {
    match app_name {
        "Safari" | "Safari Technology Preview" => {
            Some(r#"tell application "Safari" to get URL of current tab of front window"#)
        }
        "Google Chrome" | "Google Chrome Canary" | "Chromium" => {
            Some(r#"tell application "Google Chrome" to get URL of active tab of front window"#)
        }
        "Microsoft Edge" | "Microsoft Edge Beta" | "Microsoft Edge Dev" => {
            Some(r#"tell application "Microsoft Edge" to get URL of active tab of front window"#)
        }
        "Arc" => Some(r#"tell application "Arc" to get URL of active tab of front window"#),
        "Brave Browser" => {
            Some(r#"tell application "Brave Browser" to get URL of active tab of front window"#)
        }
        _ => None,
    }
}

fn get_browser_url(app_name: &str) -> Option<String> {
    let script = browser_script_for(app_name)?;
    let output = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() || url == "missing value" { None } else { Some(url) }
}

impl EventHandler {
    fn handle_files(&self, files: Vec<String>, source: SourceInfo) {
        let files: Vec<String> = files
            .into_iter()
            .filter(|path| !path.trim().is_empty())
            .collect();
        if files.is_empty() {
            return;
        }

        // 多个路径按剪贴板给出的顺序用 \n 连接后一起参与指纹 ——
        // 同一批文件去重；顺序不同算不同内容（顺序本身是有意义的信息）。
        let joined = files.join("\n");
        let fingerprint = compute_fingerprint("files", &joined);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }

        self.send_event(ClipboardEvent {
            content_type: ContentType::Files,
            content_text: joined,
            source_app: source.app_name,
            source_url: source.source_url,
        });
    }

    fn handle_text(&self, text: String, source: SourceInfo) {
        let fingerprint = compute_fingerprint("text", &text);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }
        self.send_event(ClipboardEvent {
            content_type: ContentType::Text,
            content_text: text,
            source_app: source.app_name,
            source_url: source.source_url,
        });
    }

    fn handle_html(&self, html: String, source: SourceInfo) {
        let fingerprint = compute_fingerprint("html", &html);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }
        self.send_event(ClipboardEvent {
            content_type: ContentType::Html,
            content_text: html,
            source_app: source.app_name,
            source_url: source.source_url,
        });
    }

    fn handle_image(&self, ctx: &ClipboardContext, source: SourceInfo) {
        let image = match ctx.get_image() {
            Ok(img) => img,
            Err(_) => return,
        };

        if image.is_empty() {
            return;
        }

        let png_buf = match image.to_png() {
            Ok(buf) => buf,
            Err(err) => {
                tracing::debug!(error = %err, "图片转 PNG 失败，跳过");
                return;
            }
        };

        let png_bytes = png_buf.get_bytes();
        if png_bytes.is_empty() {
            return;
        }

        let fingerprint = compute_fingerprint_bytes("image", png_bytes);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }

        // 用 fingerprint 前 32 字符做文件名，同内容不重复写文件
        let file_name = format!("{}.png", &fingerprint[..32.min(fingerprint.len())]);
        let file_path = self.image_dir.join(&file_name);

        if !file_path.exists() {
            if let Err(err) = std::fs::write(&file_path, png_bytes) {
                tracing::warn!(error = %err, "保存截图到文件失败");
                return;
            }
        }

        self.send_event(ClipboardEvent {
            content_type: ContentType::Image,
            content_text: file_path.to_string_lossy().to_string(),
            source_app: source.app_name,
            source_url: source.source_url,
        });
    }

    fn send_event(&self, event: ClipboardEvent) {
        let sender = self.sender.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = sender.send(event).await {
                tracing::warn!(error = %err, "剪贴板事件发送失败");
            }
        });
    }
}

// ---------------------------------------------------------------------------
// ClipboardSource
// ---------------------------------------------------------------------------

pub struct MacOsClipboardSource {
    self_write_guard: SelfWriteGuard,
    image_dir: PathBuf,
    running: Arc<AtomicBool>,
    shutdown: Option<clipboard_rs::WatcherShutdown>,
}

impl MacOsClipboardSource {
    pub fn new(self_write_guard: SelfWriteGuard, image_dir: PathBuf) -> Self {
        Self {
            self_write_guard,
            image_dir,
            running: Arc::new(AtomicBool::new(false)),
            shutdown: None,
        }
    }
}

impl ClipboardSource for MacOsClipboardSource {
    fn start(&mut self, sender: Sender<ClipboardEvent>) -> Result<(), ClipboardSourceError> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let self_write_guard = self.self_write_guard.clone();
        let image_dir = self.image_dir.clone();
        let running = self.running.clone();

        let mut watcher = ClipboardWatcherContext::new()
            .map_err(|e| ClipboardSourceError::ReadFailed(e.to_string()))?;

        let handler = EventHandler {
            sender,
            self_write_guard,
            image_dir,
        };
        watcher.add_handler(handler);
        let shutdown = watcher.get_shutdown_channel();
        self.shutdown = Some(shutdown);
        running.store(true, Ordering::SeqCst);

        thread::Builder::new()
            .name("clipmaster-clipboard-watcher".into())
            .spawn(move || {
                watcher.start_watch();
                running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| ClipboardSourceError::ReadFailed(e.to_string()))?;

        Ok(())
    }

    fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.stop();
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// ClipboardWriter
// ---------------------------------------------------------------------------

/// 把 HTML 组装成剪贴板的 representations：HTML 必带，纯文本兜底仅在
/// 剥离后仍有可读内容时附带。
///
/// 必须用 `ctx.set(contents)` **一次**写入。clipboard-rs 在 macOS 上的
/// `set_html`/`set_text` 是各自「先 clearContents() 再写」，分两次调用
/// 第二次会把第一次的 representation 整个清掉 —— 粘贴退化成纯文本；
/// 单次 `set` 才是同一个 NSPasteboardItem 挂多个 representation。
fn clipboard_contents_for_html(html: &str, plain: &str) -> Vec<ClipboardContent> {
    let mut contents = vec![ClipboardContent::Html(html.to_string())];
    if !plain.trim().is_empty() {
        contents.push(ClipboardContent::Text(plain.to_string()));
    }
    contents
}

/// HTML 写入的自写守护指纹：与 [`clipboard_contents_for_html`] 产出的
/// representation 一一对应 —— 多一份内容就多一个指纹，少一份就少一个。
fn html_write_fingerprints(html: &str, plain: &str) -> Vec<String> {
    let mut fingerprints = vec![compute_fingerprint("html", html)];
    if !plain.trim().is_empty() {
        fingerprints.push(compute_fingerprint("text", plain));
    }
    fingerprints
}

pub struct MacOsClipboardWriter {
    self_write_guard: SelfWriteGuard,
}

impl MacOsClipboardWriter {
    pub fn new(self_write_guard: SelfWriteGuard) -> Self {
        Self { self_write_guard }
    }
}

impl ClipboardWriter for MacOsClipboardWriter {
    fn write_text(&self, content: &str) -> Result<(), ClipboardSourceError> {
        let fingerprint = compute_fingerprint("text", content);
        mark_self_write(&self.self_write_guard, &[fingerprint]);
        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set_text(content.to_string())
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        Ok(())
    }

    fn write_image(&self, image_path: &str) -> Result<(), ClipboardSourceError> {
        let png_bytes = std::fs::read(image_path)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("读取图片文件失败: {e}")))?;

        let fingerprint = compute_fingerprint_bytes("image", &png_bytes);
        mark_self_write(&self.self_write_guard, &[fingerprint]);

        // RustImageData::from_bytes 内部用 image crate 解码 PNG → DynamicImage
        let image_data = RustImageData::from_bytes(&png_bytes)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("PNG 解码失败: {e}")))?;

        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set_image(image_data)
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        Ok(())
    }

    fn write_html(&self, html: &str) -> Result<(), ClipboardSourceError> {
        if html.trim().is_empty() {
            return Err(ClipboardSourceError::WriteFailed("HTML 内容为空".to_string()));
        }

        // 同时带一份纯文本兜底：目标应用若只认 text/plain（终端、多数输入框），
        // 粘出来的也是可读内容，而不是什么都没有。两个 representation 必须
        // 单次 set 写入（见 clipboard_contents_for_html 的注释）。
        let plain = strip_html_tags(html);
        let contents = clipboard_contents_for_html(html, &plain);
        mark_self_write(&self.self_write_guard, &html_write_fingerprints(html, &plain));

        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set(contents)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("写入剪贴板失败: {e}")))
    }

    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardSourceError> {
        let paths: Vec<String> = paths
            .iter()
            .filter(|path| !path.trim().is_empty())
            .cloned()
            .collect();
        if paths.is_empty() {
            return Err(ClipboardSourceError::WriteFailed("文件列表为空".to_string()));
        }

        // 指纹口径必须与采集侧一致：同样按顺序用 \n 连接后计算。
        let fingerprint = compute_fingerprint("files", &paths.join("\n"));
        mark_self_write(&self.self_write_guard, &[fingerprint]);

        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set_files(paths)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("写入文件引用失败: {e}")))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_write_guard_is_consumed_after_first_match() {
        let guard = new_self_write_guard();
        mark_self_write(&guard, &["fp-1".to_string()]);
        assert!(matches_and_consume_self_write(&guard, "fp-1"));
        assert!(guard.lock().unwrap().is_empty());
        assert!(!matches_and_consume_self_write(&guard, "fp-1"));
    }

    #[test]
    fn self_write_guard_does_not_match_different_fingerprint() {
        let guard = new_self_write_guard();
        mark_self_write(&guard, &["fp-1".to_string()]);
        assert!(!matches_and_consume_self_write(&guard, "fp-2"));
        assert_eq!(guard.lock().unwrap().as_slice(), ["fp-1".to_string()]);
    }

    #[test]
    fn html_write_marks_both_html_and_plain_text_fingerprints() {
        // 写 HTML 时同时放两份内容进剪贴板；采集侧无论先读到哪份都得能对上。
        let guard = new_self_write_guard();
        mark_self_write(
            &guard,
            &["fp-html".to_string(), "fp-plain".to_string()],
        );

        // 采集侧先读到纯文本 → 消费纯文本指纹；
        // HTML 指纹仍在队列里 —— 若目标应用清掉了 text，采集侧后来读到
        // text/html 时依然能对上，不会把回写当成用户的新复制。
        assert!(matches_and_consume_self_write(&guard, "fp-plain"));
        assert!(matches_and_consume_self_write(&guard, "fp-html"));
        assert!(guard.lock().unwrap().is_empty());
        assert!(!matches_and_consume_self_write(&guard, "fp-plain"));
    }

    #[test]
    fn self_write_queue_caps_at_limit_dropping_oldest() {
        let guard = new_self_write_guard();
        let fingerprints: Vec<String> = (0..12).map(|i| format!("fp-{i}")).collect();
        mark_self_write(&guard, &fingerprints);

        let queued = guard.lock().unwrap().clone();
        assert_eq!(queued.len(), 8);
        assert_eq!(queued[0], "fp-4", "超出上限时丢掉最旧的");
        assert_eq!(queued[7], "fp-11");
    }

    #[test]
    fn html_write_contents_carry_both_representations_for_one_set() {
        // Bugbot P1 回归：富文本写回必须把 Html 和 Text 组装成一个向量、
        // 单次 ctx.set 写入。clipboard-rs 的 set_html/set_text 各自先
        // clearContents() 再写，分两次调用第二次会把第一次清掉，
        // 粘贴就退化成纯文本了。
        let contents = clipboard_contents_for_html("<b>你好</b> 世界", "你好 世界");

        assert_eq!(contents.len(), 2, "有可读纯文本时必须是 Html + Text 两个 representation");
        match &contents[0] {
            ClipboardContent::Html(data) => assert_eq!(data.as_str(), "<b>你好</b> 世界"),
            _ => panic!("第一项应是 HTML representation"),
        }
        match &contents[1] {
            ClipboardContent::Text(data) => assert_eq!(data.as_str(), "你好 世界"),
            _ => panic!("第二项应是纯文本 representation"),
        }
    }

    #[test]
    fn html_write_skips_plain_text_fallback_when_nothing_readable() {
        // HTML 剥离后没有可读文本（纯图片、纯样式标签）时不能附带空白
        // text representation —— 有些应用优先拿 text，粘出来就是空的。
        for html in ["<img src='x.png'/>", "<div><br/></div>", "<span>  </span>"] {
            let plain = strip_html_tags(html);
            assert!(plain.trim().is_empty(), "用例 {html} 的剥离结果应为空白");
            let contents = clipboard_contents_for_html(html, &plain);
            assert_eq!(contents.len(), 1, "html = {html} 不应附带空白纯文本");
            assert!(matches!(&contents[0], ClipboardContent::Html(_)));
        }
    }

    #[test]
    fn html_write_fingerprints_match_written_representations() {
        // 自写守护登记的指纹必须与实际写进剪贴板的 representation 一一对应：
        // 空纯文本分支只写 HTML，就只登记 HTML 指纹；否则守护要么脏指纹堆积，
        // 要么在「HTML 有、纯文本无」的真实内容上漏拦。
        let html = "<b>你好</b> 世界";
        let plain = strip_html_tags(html);
        let contents = clipboard_contents_for_html(html, &plain);
        let fingerprints = html_write_fingerprints(html, &plain);

        assert_eq!(fingerprints.len(), contents.len());
        for (content, fingerprint) in contents.iter().zip(&fingerprints) {
            let actual = match content {
                ClipboardContent::Html(data) => compute_fingerprint("html", data.as_str()),
                ClipboardContent::Text(data) => compute_fingerprint("text", data.as_str()),
                _ => panic!("只应有 Html/Text 两种 representation"),
            };
            assert_eq!(&actual, fingerprint, "指纹必须对应实际写入的内容");
        }

        // 纯文本为空白时：一份 representation、一个 HTML 指纹。
        assert_eq!(clipboard_contents_for_html(html, "   ").len(), 1);
        assert_eq!(html_write_fingerprints(html, "   ").len(), 1);
    }

    #[test]
    fn browser_scripts_only_spawn_osascript_for_supported_browsers() {
        // 受支持的浏览器：脚本必须指向对应的应用（一次 URL 查询 = 一次子进程，映射错了就白付）
        assert!(browser_script_for("Safari").unwrap().contains("Safari"));
        assert!(browser_script_for("Safari Technology Preview").unwrap().contains("Safari"));
        assert!(browser_script_for("Google Chrome").unwrap().contains("Google Chrome"));
        assert!(browser_script_for("Google Chrome Canary").unwrap().contains("Google Chrome"));
        assert!(browser_script_for("Chromium").unwrap().contains("Google Chrome"));
        assert!(browser_script_for("Microsoft Edge Dev").unwrap().contains("Microsoft Edge"));
        assert!(browser_script_for("Arc").unwrap().contains("Arc"));
        assert!(browser_script_for("Brave Browser").unwrap().contains("Brave Browser"));

        // 非浏览器必须返回 None —— 每次复制都会路过这里，多 spawn 一个子进程都是浪费
        assert_eq!(browser_script_for("Finder"), None);
        assert_eq!(browser_script_for("IntelliJ IDEA"), None);
        assert_eq!(browser_script_for("iTerm2"), None);
        // 别让「包含」关系误命中：ChromeHelper 不是浏览器
        assert_eq!(browser_script_for("ChromeHelper"), None);
    }
}
