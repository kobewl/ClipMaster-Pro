//! 非 macOS 平台的占位实现：明确报错，不做"悄悄降级成明文文件"这种选择。
//!
//! 本应用当前只在 macOS 上发布（剪贴板采集、图标、粘贴都是 AppKit 专属）。
//! 这个实现存在的意义是让别的平台也能编译通过，而不是真的可用。

use async_trait::async_trait;

use crate::domain::error::SecretError;
use crate::domain::ports::SecretStore;

pub struct UnsupportedSecretStore;

impl UnsupportedSecretStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UnsupportedSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

fn unavailable() -> SecretError {
    SecretError::Unavailable("当前平台没有可用的系统钥匙串".to_string())
}

#[async_trait]
impl SecretStore for UnsupportedSecretStore {
    async fn get(&self, _account: &str) -> Result<Option<String>, SecretError> {
        Err(unavailable())
    }

    async fn set(&self, _account: &str, _value: &str) -> Result<(), SecretError> {
        Err(unavailable())
    }

    async fn delete(&self, _account: &str) -> Result<(), SecretError> {
        Err(unavailable())
    }
}
