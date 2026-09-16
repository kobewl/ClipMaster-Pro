//! 非 macOS 平台的占位实现。
//!
//! `0.01 Beta` 只承诺 macOS（ADR-003）。这里保留接口实现是为了让工程骨架
//! 在其他平台上仍可编译，但不代表 Windows/Linux 已进入验收范围。

use tokio::sync::mpsc::Sender;

use crate::domain::error::ClipboardSourceError;
use crate::domain::ports::{ClipboardEvent, ClipboardSource, ClipboardWriter};

pub struct UnsupportedClipboardSource;

impl ClipboardSource for UnsupportedClipboardSource {
    fn start(&mut self, _sender: Sender<ClipboardEvent>) -> Result<(), ClipboardSourceError> {
        Err(ClipboardSourceError::ReadFailed(
            "当前平台的剪贴板采集尚未实现，0.01 Beta 只承诺 macOS".to_string(),
        ))
    }

    fn stop(&mut self) {}
}

pub struct UnsupportedClipboardWriter;

impl ClipboardWriter for UnsupportedClipboardWriter {
    fn write_text(&self, _content: &str) -> Result<(), ClipboardSourceError> {
        Err(ClipboardSourceError::WriteFailed(
            "当前平台的剪贴板写入尚未实现，0.01 Beta 只承诺 macOS".to_string(),
        ))
    }
}
