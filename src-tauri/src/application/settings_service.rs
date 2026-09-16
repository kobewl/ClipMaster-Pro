//! 设置相关用例：读取、更新（校验失败保留原值，FR-SET-006）。

use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::ports::SettingsStore;
use crate::domain::settings::AppSettings;

pub struct SettingsService {
    store: Arc<dyn SettingsStore>,
}

impl SettingsService {
    pub fn new(store: Arc<dyn SettingsStore>) -> Self {
        Self { store }
    }

    pub async fn get(&self) -> Result<AppSettings, AppError> {
        Ok(self.store.load().await?)
    }

    pub async fn update(&self, next: AppSettings) -> Result<AppSettings, AppError> {
        next.validate().map_err(AppError::InvalidSettings)?;
        self.store.save(next.clone()).await?;
        Ok(next)
    }

    pub async fn set_capture_enabled(&self, enabled: bool) -> Result<AppSettings, AppError> {
        let mut current = self.store.load().await?;
        current.capture_enabled = enabled;
        self.store.save(current.clone()).await?;
        Ok(current)
    }
}
