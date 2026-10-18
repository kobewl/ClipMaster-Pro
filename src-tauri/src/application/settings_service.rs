//! 设置相关用例：读取、更新（校验失败保留原值，FR-SET-006）。

use std::sync::Arc;

use crate::domain::capture_policy::normalize_excluded_apps;
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

    pub async fn update(&self, mut next: AppSettings) -> Result<AppSettings, AppError> {
        next.excluded_apps = normalize_excluded_apps(next.excluded_apps);
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

    /// 「不记录这些应用」名单单独一条立即生效的通道（与快捷键、主题一样不等「保存」）。
    pub async fn set_excluded_apps(&self, apps: Vec<String>) -> Result<AppSettings, AppError> {
        let mut current = self.store.load().await?;
        current.excluded_apps = normalize_excluded_apps(apps);
        current.validate().map_err(AppError::InvalidSettings)?;
        self.store.save(current.clone()).await?;
        Ok(current)
    }
}
