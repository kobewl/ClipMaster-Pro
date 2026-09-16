//! 分组管理用例：CRUD。

use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::model::ClipGroup;
use crate::domain::ports::GroupRepository;

pub struct GroupService {
    repository: Arc<dyn GroupRepository>,
}

impl GroupService {
    pub fn new(repository: Arc<dyn GroupRepository>) -> Self {
        Self { repository }
    }

    pub async fn list(&self) -> Result<Vec<ClipGroup>, AppError> {
        Ok(self.repository.list_groups().await?)
    }

    pub async fn create(&self, name: String, color: String) -> Result<ClipGroup, AppError> {
        if name.trim().is_empty() {
            return Err(AppError::InvalidSettings("分组名称不能为空".to_string()));
        }
        Ok(self.repository.create_group(name, color).await?)
    }

    pub async fn update(
        &self,
        id: String,
        name: String,
        color: String,
    ) -> Result<ClipGroup, AppError> {
        if name.trim().is_empty() {
            return Err(AppError::InvalidSettings("分组名称不能为空".to_string()));
        }
        Ok(self.repository.update_group(id, name, color).await?)
    }

    pub async fn delete(&self, id: String) -> Result<(), AppError> {
        Ok(self.repository.delete_group(id).await?)
    }
}
