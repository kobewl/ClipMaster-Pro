//! Composition Root。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::application::agent_service::{AgentService, REQUEST_COOLDOWN};
use crate::application::capture_pipeline::CapturePipeline;
use crate::application::group_service::GroupService;
use crate::application::history_service::HistoryService;
use crate::application::settings_service::SettingsService;
use crate::domain::error::AppError;
use crate::domain::ports::SettingsStore;
use crate::domain::settings::AppSettings;
use crate::infrastructure::sqlite::agent_run_store::SqliteAgentRunStore;
use crate::infrastructure::sqlite::group_repository::SqliteGroupRepository;
use crate::infrastructure::sqlite::repository::SqliteClipboardRepository;
use crate::infrastructure::sqlite::settings_store::SqliteSettingsStore;

#[cfg(target_os = "macos")]
use crate::infrastructure::clipboard::macos::{
    new_self_write_guard, MacOsClipboardSource, MacOsClipboardWriter,
};
#[cfg(not(target_os = "macos"))]
use crate::infrastructure::clipboard::unsupported::{
    UnsupportedClipboardSource, UnsupportedClipboardWriter,
};

// 模型密钥的系统安全存储：macOS 钥匙串 / 其它平台明确不可用。
#[cfg(target_os = "macos")]
use crate::infrastructure::secret::macos::KeychainSecretStore as PlatformSecretStore;
#[cfg(not(target_os = "macos"))]
use crate::infrastructure::secret::unsupported::UnsupportedSecretStore as PlatformSecretStore;

pub struct AppRuntime {
    pub history: Arc<HistoryService>,
    pub agent: Arc<AgentService>,
    pub settings: Arc<SettingsService>,
    pub groups: Arc<GroupService>,
    pub capture_pipeline: std::sync::Mutex<CapturePipeline>,
    settings_store: Arc<dyn SettingsStore>,
}

impl AppRuntime {
    pub fn settings_store(&self) -> &dyn SettingsStore {
        self.settings_store.as_ref()
    }

    /// 采集开关的**唯一写入路径**。设置面板保存、状态栏点击、托盘菜单
    /// 三条入口都汇到这里：settings 表是唯一真相源，pipeline 的原子标记、
    /// 前端状态栏、托盘菜单文案都是它的镜像（由 [`sync_capture_side_effects`]
    /// 统一刷新，任何一条路单独改镜像都会造成显示分叉）。
    pub async fn set_capture_enabled(
        &self,
        app: &AppHandle,
        enabled: bool,
    ) -> Result<AppSettings, AppError> {
        let updated = self.settings.set_capture_enabled(enabled).await?;
        sync_capture_side_effects(app, self, updated.capture_enabled);
        Ok(updated)
    }

    pub fn shutdown(&self) {
        if let Ok(mut pipeline) = self.capture_pipeline.lock() {
            pipeline.stop();
        }
    }
}

/// 采集开关落库后的全部联动：pipeline 内存投影、前端事件、托盘菜单文案。
pub fn sync_capture_side_effects(app: &AppHandle, runtime: &AppRuntime, capture_enabled: bool) {
    if let Ok(pipeline) = runtime.capture_pipeline.lock() {
        pipeline.set_enabled(capture_enabled);
    }
    if let Err(err) = app.emit("settings://capture-changed", capture_enabled) {
        tracing::warn!(error = %err, "发送 settings://capture-changed 事件失败");
    }
    crate::lifecycle::tray::update_capture_menu_item(app, capture_enabled);
}

pub fn build_runtime(app_handle: &AppHandle) -> Result<AppRuntime, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法解析应用数据目录: {e}"))?;
    let db_path = app_data_dir.join("clipmaster.db");

    let conn = crate::infrastructure::sqlite::open_and_migrate(&db_path)
        .map_err(|e| format!("数据库初始化失败: {e}"))?;

    let repository: Arc<dyn crate::domain::ports::ClipboardRepository> =
        Arc::new(SqliteClipboardRepository::new(conn.clone()));
    let group_repository: Arc<dyn crate::domain::ports::GroupRepository> =
        Arc::new(SqliteGroupRepository::new(conn.clone()));
    // 同一个对象当两种端口用：设置项和模型服务配置都存在 app_settings 表里。
    let settings_impl = Arc::new(SqliteSettingsStore::new(conn.clone()));
    let settings_store: Arc<dyn SettingsStore> = settings_impl.clone();

    let image_dir = app_data_dir.join("images");
    std::fs::create_dir_all(&image_dir).map_err(|e| format!("创建图片存储目录失败: {e}"))?;

    #[cfg(target_os = "macos")]
    let (writer, source): (
        Arc<dyn crate::domain::ports::ClipboardWriter>,
        Box<dyn crate::domain::ports::ClipboardSource>,
    ) = {
        let guard = new_self_write_guard();
        (
            Arc::new(MacOsClipboardWriter::new(guard.clone())),
            Box::new(MacOsClipboardSource::new(guard, image_dir)),
        )
    };

    #[cfg(not(target_os = "macos"))]
    let (writer, source): (
        Arc<dyn crate::domain::ports::ClipboardWriter>,
        Box<dyn crate::domain::ports::ClipboardSource>,
    ) = (
        Arc::new(UnsupportedClipboardWriter),
        Box::new(UnsupportedClipboardSource),
    );

    let history = Arc::new(HistoryService::new(
        repository.clone(),
        writer,
        settings_store.clone(),
    ));
    let secrets: Arc<dyn crate::domain::ports::SecretStore> = Arc::new(PlatformSecretStore::new());
    let run_store: Arc<dyn crate::domain::ports::AgentRunStore> =
        Arc::new(SqliteAgentRunStore::new(conn.clone()));
    // 唯一的构造点：单飞闸门在 AgentService 内部，冷却只在这里开启 ——
    // 测试一律走 `new()` 默认的 0，不被节流干扰。
    let agent = Arc::new(
        AgentService::new(history.clone(), secrets, settings_impl, run_store)
            .with_cooldown(REQUEST_COOLDOWN),
    );
    let settings = Arc::new(SettingsService::new(settings_store.clone()));
    let groups = Arc::new(GroupService::new(group_repository));

    spawn_retention_cleanup_timer(history.clone(), settings_store.clone());

    let mut capture_pipeline = CapturePipeline::new(source);

    let app_handle_for_events = app_handle.clone();
    if let Err(err) = capture_pipeline.start(history.clone(), move |item| {
        if let Err(emit_err) = app_handle_for_events.emit("clipboard://captured", &item) {
            tracing::warn!(error = %emit_err, "发送 clipboard://captured 事件失败");
        }
    }) {
        tracing::warn!(error = %err, "启动剪贴板采集失败");
        if let Err(emit_err) = app_handle.emit(
            "app://error",
            serde_json::json!({
                "code": "clipboard_capture_unavailable",
                "message": err.to_string(),
            }),
        ) {
            tracing::warn!(error = %emit_err, "发送 app://error 事件失败");
        }
    }

    Ok(AppRuntime {
        history,
        agent,
        settings,
        groups,
        capture_pipeline: std::sync::Mutex::new(capture_pipeline),
        settings_store,
    })
}

fn spawn_retention_cleanup_timer(
    history: Arc<HistoryService>,
    settings_store: Arc<dyn SettingsStore>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let settings = match settings_store.load().await {
                Ok(s) => s,
                Err(err) => {
                    tracing::warn!(error = %err, "定时清理读取设置失败");
                    continue;
                }
            };
            if let Err(err) = history.run_retention_cleanup(settings.retention_days).await {
                tracing::warn!(error = %err, "定时按天保留清理失败");
            }
        }
    });
}
