//! macOS 剪贴板采集与写入实现。
//!
//! 支持 Text + Image 两种类型。
//! 防自拷贝循环：writer 写入前记录 fingerprint，采集侧匹配后跳过。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher,
    ClipboardWatcherContext, RustImageData,
};
use tokio::sync::mpsc::Sender;

use crate::domain::error::ClipboardSourceError;
use crate::domain::model::ContentType;
use crate::domain::normalize::{compute_fingerprint, compute_fingerprint_bytes};
use crate::domain::ports::{ClipboardEvent, ClipboardSource, ClipboardWriter};

pub type SelfWriteGuard = Arc<Mutex<Option<String>>>;

pub fn new_self_write_guard() -> SelfWriteGuard {
    Arc::new(Mutex::new(None))
}

fn matches_and_consume_self_write(guard: &SelfWriteGuard, fingerprint: &str) -> bool {
    let mut guard = guard.lock().expect("mutex poisoned");
    if guard.as_deref() == Some(fingerprint) {
        *guard = None;
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

        // 优先尝试文本
        if let Ok(text) = ctx.get_text() {
            if !text.is_empty() {
                self.handle_text(text, source_info);
                return;
            }
        }

        // 文本为空或获取失败时尝试图片
        self.handle_image(&ctx, source_info);
    }
}

struct SourceInfo {
    app_name: Option<String>,
    source_url: Option<String>,
}

fn get_source_info() -> SourceInfo {
    let app_name = get_frontmost_app_name();
    let source_url = app_name.as_ref().and_then(|name| get_browser_url(name));
    SourceInfo { app_name, source_url }
}

fn get_frontmost_app_name() -> Option<String> {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of first process whose frontmost is true",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn get_browser_url(app_name: &str) -> Option<String> {
    let script = match app_name {
        "Safari" | "Safari Technology Preview" => {
            r#"tell application "Safari" to get URL of current tab of front window"#
        }
        "Google Chrome" | "Google Chrome Canary" | "Chromium" => {
            r#"tell application "Google Chrome" to get URL of active tab of front window"#
        }
        "Microsoft Edge" | "Microsoft Edge Beta" | "Microsoft Edge Dev" => {
            r#"tell application "Microsoft Edge" to get URL of active tab of front window"#
        }
        "Arc" => {
            r#"tell application "Arc" to get URL of active tab of front window"#
        }
        "Brave Browser" => {
            r#"tell application "Brave Browser" to get URL of active tab of front window"#
        }
        _ => return None,
    };
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
        {
            let mut guard = self.self_write_guard.lock().expect("mutex poisoned");
            *guard = Some(fingerprint);
        }
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
        {
            let mut guard = self.self_write_guard.lock().expect("mutex poisoned");
            *guard = Some(fingerprint);
        }

        // RustImageData::from_bytes 内部用 image crate 解码 PNG → DynamicImage
        let image_data = RustImageData::from_bytes(&png_bytes)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("PNG 解码失败: {e}")))?;

        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set_image(image_data)
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
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
        *guard.lock().unwrap() = Some("fp-1".to_string());
        assert!(matches_and_consume_self_write(&guard, "fp-1"));
        assert!(guard.lock().unwrap().is_none());
        assert!(!matches_and_consume_self_write(&guard, "fp-1"));
    }

    #[test]
    fn self_write_guard_does_not_match_different_fingerprint() {
        let guard = new_self_write_guard();
        *guard.lock().unwrap() = Some("fp-1".to_string());
        assert!(!matches_and_consume_self_write(&guard, "fp-2"));
        assert_eq!(guard.lock().unwrap().as_deref(), Some("fp-1"));
    }
}
