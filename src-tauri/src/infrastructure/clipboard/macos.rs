//! macOS 剪贴板采集与写入实现。
//!
//! 参考文档：
//! - 02-架构与设计/02-macOS平台设计.md 第 3 节：基于变更计数轮询，
//!   变更计数未变化时不读取具体内容，读取和持久化在异步流水线中完成。
//! - clipboard-rs 的 `ClipboardWatcherContext` 内部即基于 macOS `NSPasteboard.changeCount`
//!   轮询实现（跨平台库对 macOS 的实现方式），本模块直接复用其 watcher。
//!
//! 防自拷贝循环策略（架构文档 8 节 + macOS 设计文档 3 节“记录内部 generation/fingerprint”）：
//! 应用通过 `ClipboardWriter::write_text` 写回剪贴板前，会把即将写入的文本的 fingerprint
//! 记录到共享的 `last_written_fingerprint`。采集侧在处理新事件时，若新内容的 fingerprint
//! 与该值相同，则判定为“自己刚写的”，直接跳过，不重新入库。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
};
use tokio::sync::mpsc::Sender;

use crate::domain::error::ClipboardSourceError;
use crate::domain::normalize::compute_fingerprint;
use crate::domain::ports::{ClipboardEvent, ClipboardSource, ClipboardWriter};

/// 应用自身最近一次写入剪贴板内容的 fingerprint，用于防自拷贝循环判断。
/// 使用 `Arc<Mutex<..>>` 在 writer 和 source 之间共享。
pub type SelfWriteGuard = Arc<Mutex<Option<String>>>;

pub fn new_self_write_guard() -> SelfWriteGuard {
    Arc::new(Mutex::new(None))
}

/// 检查剪贴板变化事件的 fingerprint 是否等于应用自己最近一次写入的值。
/// 匹配后立即清除 guard（置回 `None`），使其只吞掉“自己刚写的那一次”变化，
/// 不会影响后续任何人（包括用户从外部）再复制相同内容时的正常入库。
///
/// 之前的实现只读不清，guard 里的旧 fingerprint 会一直留着，导致用户之后
/// 从别处复制到完全相同的文本时也被误判为“自己写的”而被跳过。
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

        // 首版只支持文本类型（ADR-004），非文本内容直接忽略，不产生错误噪音。
        let text = match ctx.get_text() {
            Ok(text) => text,
            Err(_) => return,
        };

        if text.is_empty() {
            // FR-CAP: 空文本不入库。
            return;
        }

        let fingerprint = compute_fingerprint("text", &text);

        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            // 应用自己刚写回的内容，跳过一次，避免无意义重复记录（FR-CAP-003）。
            return;
        }

        // source_app 为 P1/可选字段（产品需求文档 FR-CAP、隐私规范“数据最小化”）。
        // `0.01 Beta` 采集路径暂不识别来源应用，避免引入额外的 NSRunLoop 依赖
        // 与 Tauri 自身事件循环产生冲突；后续若需要，应通过 ADR 评估权限与实现方式。
        let event = ClipboardEvent {
            content_text: text,
            source_app: None,
        };

        let sender = self.sender.clone();
        // 使用 Tauri 的全局 async runtime handle，而不是 `tokio::runtime::Handle::current()`：
        // `setup()` 钩子和 watcher 线程都不在某个活跃 tokio task 内执行，
        // `Handle::current()` 在这种上下文下会直接 panic。
        tauri::async_runtime::spawn(async move {
            if let Err(err) = sender.send(event).await {
                tracing::warn!(error = %err, "剪贴板事件发送失败，接收端可能已关闭");
            }
        });
    }
}

pub struct MacOsClipboardSource {
    self_write_guard: SelfWriteGuard,
    running: Arc<AtomicBool>,
    shutdown: Option<clipboard_rs::WatcherShutdown>,
}

impl MacOsClipboardSource {
    pub fn new(self_write_guard: SelfWriteGuard) -> Self {
        Self {
            self_write_guard,
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
        let running = self.running.clone();

        let mut watcher = ClipboardWatcherContext::new()
            .map_err(|e| ClipboardSourceError::ReadFailed(e.to_string()))?;

        let handler = EventHandler {
            sender,
            self_write_guard,
        };
        watcher.add_handler(handler);
        let shutdown = watcher.get_shutdown_channel();
        self.shutdown = Some(shutdown);

        running.store(true, Ordering::SeqCst);

        // `start_watch` 是阻塞调用，必须放在独立线程中运行（clipboard-rs 文档要求）。
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_write_guard_is_consumed_after_first_match() {
        let guard = new_self_write_guard();
        *guard.lock().unwrap() = Some("fp-1".to_string());

        // 第一次匹配：应命中并清除 guard。
        assert!(matches_and_consume_self_write(&guard, "fp-1"));
        assert!(guard.lock().unwrap().is_none(), "命中后必须清空 guard");

        // 之前的 bug：guard 被清空前会一直匹配同一个 fingerprint，导致用户后续
        // 从外部复制完全相同的内容也被误判为“自己写的”而跳过。
        // 修复后：guard 已被清空，同样的 fingerprint 不应再被判定为自写。
        assert!(
            !matches_and_consume_self_write(&guard, "fp-1"),
            "guard 消耗后，相同 fingerprint 不应再被误判为自写"
        );
    }

    #[test]
    fn self_write_guard_does_not_match_different_fingerprint() {
        let guard = new_self_write_guard();
        *guard.lock().unwrap() = Some("fp-1".to_string());

        assert!(!matches_and_consume_self_write(&guard, "fp-2"));
        // 不匹配时不应清除 guard，后续真正的自写事件仍应能被正确识别。
        assert_eq!(guard.lock().unwrap().as_deref(), Some("fp-1"));
    }
}
