//! 敏感配置（模型 API Key）的系统安全存储。
//!
//! 目前只有 macOS 实现（系统钥匙串）；其它平台给一个明确报错的空实现，
//! 保证跨平台能编译且不会静默丢密钥。

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(target_os = "macos"))]
pub mod unsupported;
