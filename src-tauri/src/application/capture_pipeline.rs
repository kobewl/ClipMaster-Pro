//! 剪贴板采集流水线编排。
//! 对应架构文档第 6 节：OS Watcher -> Reader -> Normalizer -> CapturePolicy
//! -> Deduplicator -> CaptureClipboardItem -> Repository -> UI Event。
//!
//! 这里的 Reader/Normalizer 已经在 `infrastructure::clipboard` 中完成（只产出
//! 干净的文本），本模块负责：接收有界 channel 事件、调用 HistoryService、
//! 并将结果通过回调转发给 UI 层（Tauri Event）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::application::history_service::HistoryService;
use crate::domain::model::ClipboardItem;
use crate::domain::ports::{ClipboardEvent, ClipboardSource};

/// 有界队列容量。过载时新事件的发送方（同步 watcher 线程）会感知到 backpressure，
/// 而不是无限增长内存（架构文档第 8 节并发规则、性能需求文档 4.2 节）。
const EVENT_QUEUE_CAPACITY: usize = 64;

pub struct CapturePipeline {
    source: Box<dyn ClipboardSource>,
    capture_enabled: Arc<AtomicBool>,
}

impl CapturePipeline {
    pub fn new(source: Box<dyn ClipboardSource>) -> Self {
        Self {
            source,
            capture_enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.capture_enabled.store(enabled, Ordering::SeqCst);
    }

    /// 启动采集。`on_captured` 会在每次成功入库后被调用，用于向前端发送
    /// `clipboard://captured` 事件。
    pub fn start(
        &mut self,
        history_service: Arc<HistoryService>,
        on_captured: impl Fn(ClipboardItem) + Send + Sync + 'static,
    ) -> Result<(), crate::domain::error::ClipboardSourceError> {
        let (tx, mut rx) = mpsc::channel::<ClipboardEvent>(EVENT_QUEUE_CAPACITY);
        let capture_enabled = self.capture_enabled.clone();

        self.source.start(tx)?;

        // 与 macOS watcher 一样，这里必须使用 Tauri 的 async runtime handle，
        // 因为 `setup()` 钩子不在活跃 tokio task 上下文中执行。
        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                if !capture_enabled.load(Ordering::SeqCst) {
                    // 用户暂停采集时（FR-SYS 菜单栏“暂停/恢复采集”），
                    // 直接丢弃事件，不入库、不产生错误。
                    continue;
                }

                match history_service
                    .capture(event)
                    .await
                {
                    Ok(item) => on_captured(item),
                    Err(err) => {
                        // 单条失败不得中断整条流水线（可靠性需求 4.2 节）。
                        tracing::debug!(error = %err, "捕获剪贴板内容失败，已跳过");
                    }
                }
            }
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        self.source.stop();
    }
}
