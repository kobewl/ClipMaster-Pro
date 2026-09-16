//! Composition Root：组装 Repository、Service、CapturePipeline，
//! 并把它们放入 `AppRuntime` 供 Tauri Commands 使用（架构文档第 8 节）。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::application::capture_pipeline::CapturePipeline;
use crate::application::history_service::HistoryService;
use crate::application::settings_service::SettingsService;
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

pub struct AppRuntime {
    pub history: Arc<HistoryService>,
    pub settings: Arc<SettingsService>,
    pub capture_pipeline: std::sync::Mutex<CapturePipeline>,
    settings_store: Arc<SqliteSettingsStore>,
}

impl AppRuntime {
    /// 供 setup() 初始化阶段读取快捷键等配置项。
    pub fn settings_store(&self) -> &SqliteSettingsStore {
        &self.settings_store
    }

    /// 退出流程：停止 watcher（架构文档第 8 节退出顺序的第一步）。
    /// SQLite 连接在 `Arc` 被释放时自动关闭；Beta 阶段没有需要显式排空的
    /// 后台任务队列之外的资源。
    pub fn shutdown(&self) {
        if let Ok(mut pipeline) = self.capture_pipeline.lock() {
            pipeline.stop();
        }
    }
}

/// 启动时组装全部依赖。数据库路径使用 Tauri 提供的应用数据目录，
/// 遵守“使用操作系统推荐的应用数据目录”（安全与隐私规范第 6 节）。
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
    let settings_store = Arc::new(SqliteSettingsStore::new(conn.clone()));

    #[cfg(target_os = "macos")]
    let (writer, source): (
        Arc<dyn crate::domain::ports::ClipboardWriter>,
        Box<dyn crate::domain::ports::ClipboardSource>,
    ) = {
        let guard = new_self_write_guard();
        (
            Arc::new(MacOsClipboardWriter::new(guard.clone())),
            Box::new(MacOsClipboardSource::new(guard)),
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
    let settings = Arc::new(SettingsService::new(settings_store.clone()));

    // 按天保留清理同时依赖捕获时触发（见 HistoryService::capture_text）和这里的
    // 小时级定时任务：如果用户长时间不产生新的剪贴板事件，仅靠捕获时触发无法让
    // 过期数据被清理，用户设置的“保留 N 天”会形同虚设。定时任务与捕获时触发调用
    // 的是同一个幂等的 repository 方法，不会重复删除或产生副作用。
    spawn_retention_cleanup_timer(history.clone(), settings_store.clone());

    let mut capture_pipeline = CapturePipeline::new(source);

    let app_handle_for_events = app_handle.clone();
    if let Err(err) = capture_pipeline.start(history.clone(), move |item| {
        if let Err(emit_err) = app_handle_for_events.emit("clipboard://captured", &item) {
            tracing::warn!(error = %emit_err, "发送 clipboard://captured 事件失败");
        }
    }) {
        // 采集启动失败（例如权限缺失）不应阻止应用其余功能可用，
        // 但必须让用户能感知到（通过 app://error 事件，macOS 平台设计文档第 6 节）。
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
        settings,
        capture_pipeline: std::sync::Mutex::new(capture_pipeline),
        settings_store,
    })
}

/// 每小时执行一次按天保留清理，独立于剪贴板捕获事件。
fn spawn_retention_cleanup_timer(
    history: Arc<HistoryService>,
    settings_store: Arc<SqliteSettingsStore>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let settings = match settings_store.load().await {
                Ok(settings) => settings,
                Err(err) => {
                    tracing::warn!(error = %err, "定时清理读取设置失败，跳过本次清理");
                    continue;
                }
            };
            if let Err(err) = history.run_retention_cleanup(settings.retention_days).await {
                tracing::warn!(error = %err, "定时按天保留清理失败");
            }
        }
    });
}


