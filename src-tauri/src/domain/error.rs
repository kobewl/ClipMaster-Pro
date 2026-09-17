//! 领域与应用层统一错误类型。
//!
//! 参考架构文档 7.3 节 CommandError 契约：前端依据稳定 `code` 处理业务状态，
//! 不解析自然语言 message。这里的 `code()` 方法就是该契约的落地点。

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("不支持的内容类型: {0}")]
    InvalidContentType(String),

    #[error("内容为空，不允许保存")]
    EmptyContent,

    #[error("内容超过大小上限: {actual} 字节，上限 {limit} 字节")]
    ContentTooLarge { actual: usize, limit: usize },
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("未找到记录: {0}")]
    NotFound(String),

    #[error("数据库迁移失败: {0}")]
    Migration(String),
}

#[derive(Debug, Error)]
pub enum ClipboardSourceError {
    #[error("剪贴板读取失败: {0}")]
    ReadFailed(String),

    #[error("剪贴板写入失败: {0}")]
    WriteFailed(String),
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error(transparent)]
    Repository(#[from] RepositoryError),

    #[error(transparent)]
    Clipboard(#[from] ClipboardSourceError),

    #[error("设置无效: {0}")]
    InvalidSettings(String),
}

impl AppError {
    /// 稳定错误码，前端依赖此字段而非 message 文本（架构文档 7.3 节）。
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Domain(DomainError::InvalidContentType(_)) => "invalid_content_type",
            AppError::Domain(DomainError::EmptyContent) => "empty_content",
            AppError::Domain(DomainError::ContentTooLarge { .. }) => "content_too_large",
            AppError::Repository(RepositoryError::Database(_)) => "database_error",
            AppError::Repository(RepositoryError::NotFound(_)) => "not_found",
            AppError::Repository(RepositoryError::Migration(_)) => "migration_failed",
            AppError::Clipboard(ClipboardSourceError::ReadFailed(_)) => "clipboard_read_failed",
            AppError::Clipboard(ClipboardSourceError::WriteFailed(_)) => "clipboard_write_failed",
            AppError::InvalidSettings(_) => "invalid_settings",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            AppError::Repository(RepositoryError::Database(_))
                | AppError::Clipboard(_)
        )
    }
}

/// Tauri Command 的错误响应体，通过 `Serialize` 直接传给前端。
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<serde_json::Value>,
}

impl CommandError {
    pub fn new(code: &str, message: String, retryable: bool) -> Self {
        CommandError {
            code: code.to_string(),
            message,
            retryable,
            details: None,
        }
    }

    /// 快捷键注册/变更失败（FR-SYS-003：冲突时明确反馈）。
    pub fn shortcut(reason: String) -> Self {
        Self::new("shortcut_conflict", reason, false)
    }

    /// 「自动粘贴」这步失败。
    ///
    /// 注意：走到这里时内容**已经写进剪贴板了**，失败的只是模拟 ⌘V，
    /// 大多数情况是没给「辅助功能」权限。前端据此给用户一条明确提示，
    /// 而不是假装成功（原来的实现就是静默吞掉，用户只会觉得「点了没反应」）。
    pub fn paste(reason: String) -> Self {
        Self::new("paste_failed", reason, false)
    }

    /// 来源图标解析失败（非致命：前端会退回 emoji）。
    pub fn icon(reason: String) -> Self {
        Self::new("icon_failed", reason, true)
    }

    /// 系统集成操作失败（开机自启等）。
    ///
    /// 用 `system_` 前缀而不是复用通用错误：这类失败的原因和剪贴板、数据库都无关，
    /// 大多是被系统策略挡住（比如 LaunchAgents 目录不可写），前端要能把它们分开提示。
    pub fn system(reason: String) -> Self {
        Self::new("system_integration_failed", reason, false)
    }
}

impl From<AppError> for CommandError {
    fn from(err: AppError) -> Self {
        CommandError {
            code: err.code().to_string(),
            message: err.to_string(),
            retryable: err.retryable(),
            details: None,
        }
    }
}
