use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::dto::{AppSettingsDto, ClipboardItemDto, ListQueryDto, ListResultDto};
use crate::domain::error::CommandError;
use crate::lifecycle::runtime::AppRuntime;
use crate::lifecycle::shortcut::swap_shortcut;

/// 统一发送 UI 事件，失败只记录日志，不影响命令本身的结果
/// （事件通知是“最好有”的增强，不应因为 emit 失败而让整个命令报错）。
fn emit_or_warn<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!(error = %err, event, "发送事件失败");
    }
}

/// 单页最大条数上限，防止前端传入异常大的 limit 拖垮查询
/// （性能需求文档 4.1 节：10,000 条记录下搜索 P95 不高于 150ms）。
const MAX_PAGE_SIZE: u32 = 500;

#[tauri::command]
pub async fn list_clipboard_items(
    runtime: State<'_, AppRuntime>,
    query: ListQueryDto,
) -> Result<ListResultDto, CommandError> {
    let limit = query.limit.clamp(1, MAX_PAGE_SIZE);
    let search_query = crate::application::history_service::HistoryService::build_search_query(
        query.favorites_only,
        query.search,
        limit,
        query.offset,
    );
    let result = runtime
        .history
        .list(search_query)
        .await
        .map_err(CommandError::from)?;

    Ok(ListResultDto {
        items: result.items.into_iter().map(ClipboardItemDto::from).collect(),
        total: result.total,
    })
}

#[tauri::command]
pub async fn delete_clipboard_item(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<(), CommandError> {
    runtime.history.delete(&id).await.map_err(CommandError::from)?;
    // 前端 useClipboardHistory 订阅了 clipboard://deleted 用于自动刷新列表，
    // 之前这里从未 emit，导致删除后列表不会自动更新。
    emit_or_warn(&app, "clipboard://deleted", &id);
    Ok(())
}

#[tauri::command]
pub async fn set_favorite(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
    favorite: bool,
) -> Result<(), CommandError> {
    runtime
        .history
        .set_favorite(&id, favorite)
        .await
        .map_err(CommandError::from)?;

    // 收藏状态变化后重新读取最新条目，通过 clipboard://updated 通知前端，
    // 而不是让前端假设乐观更新一定正确。
    if let Ok(item) = runtime.history.get(&id).await {
        emit_or_warn(&app, "clipboard://updated", ClipboardItemDto::from(item));
    }
    Ok(())
}

#[tauri::command]
pub async fn copy_clipboard_item(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<(), CommandError> {
    runtime
        .history
        .copy_to_clipboard(&id)
        .await
        .map_err(CommandError::from)?;

    if let Ok(item) = runtime.history.get(&id).await {
        emit_or_warn(&app, "clipboard://updated", ClipboardItemDto::from(item));
    }
    Ok(())
}

/// 选中条目后一键粘贴：写入剪贴板 → 隐藏窗口 → 模拟 ⌘V。
/// 典型流程：用户按回车 → 调此命令 → 内容直接粘贴到之前的输入框/编辑器。
#[tauri::command]
pub async fn paste_clipboard_item(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<(), CommandError> {
    runtime
        .history
        .copy_to_clipboard(&id)
        .await
        .map_err(CommandError::from)?;

    if let Ok(item) = runtime.history.get(&id).await {
        emit_or_warn(&app, "clipboard://updated", ClipboardItemDto::from(item));
    }

    // 隐藏窗口，让之前的应用获得焦点
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    // 等待前一个应用获得焦点后模拟 Cmd+V
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    #[cfg(target_os = "macos")]
    crate::infrastructure::clipboard::macos::simulate_paste();

    Ok(())
}

#[tauri::command]
pub async fn clear_history(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    keep_favorites: bool,
) -> Result<u64, CommandError> {
    let deleted = runtime
        .history
        .clear(keep_favorites)
        .await
        .map_err(CommandError::from)?;
    // 清空历史同样需要通知前端刷新；用专门的 payload 区分“批量清空”与单条删除，
    // 前端收到后应当整体重新拉取列表而不是尝试按 id 移除。
    emit_or_warn(&app, "clipboard://cleared", deleted);
    Ok(deleted)
}

#[tauri::command]
pub async fn get_settings(
    runtime: State<'_, AppRuntime>,
) -> Result<AppSettingsDto, CommandError> {
    runtime
        .settings
        .get()
        .await
        .map(AppSettingsDto::from)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_settings(
    runtime: State<'_, AppRuntime>,
    settings: AppSettingsDto,
) -> Result<AppSettingsDto, CommandError> {
    let updated = runtime
        .settings
        .update(settings.into())
        .await
        .map_err(CommandError::from)?;

    if let Ok(pipeline) = runtime.capture_pipeline.lock() {
        pipeline.set_enabled(updated.capture_enabled);
    }

    Ok(AppSettingsDto::from(updated))
}

/// 独立的快捷键更新命令（FR-SET-003）。
/// 与 `update_settings` 分开的原因：快捷键变更需要和 OS 交互（注册/注销全局快捷键），
/// 失败场景（冲突、权限不足等）和普通设置不同，混在一起会使错误处理模糊。
/// 前端在录入完快捷键后单独调用这个命令，如果失败可以显示具体冲突原因。
#[tauri::command]
pub async fn update_shortcut(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    shortcut: String,
) -> Result<AppSettingsDto, CommandError> {
    let old_settings = runtime.settings.get().await.map_err(CommandError::from)?;

    // 先向 OS 注册新快捷键（成功后才注销旧的），失败立即返回具体原因。
    swap_shortcut(&app, &old_settings.shortcut, &shortcut)
        .map_err(CommandError::shortcut)?;

    // OS 注册成功后才持久化到数据库，保证"数据库里的值一定在 OS 层已生效"。
    let mut new_settings = old_settings;
    new_settings.shortcut = shortcut;
    let saved = runtime
        .settings
        .update(new_settings)
        .await
        .map_err(CommandError::from)?;

    Ok(AppSettingsDto::from(saved))
}

#[tauri::command]
pub async fn set_capture_enabled(
    runtime: State<'_, AppRuntime>,
    enabled: bool,
) -> Result<AppSettingsDto, CommandError> {
    let updated = runtime
        .settings
        .set_capture_enabled(enabled)
        .await
        .map_err(CommandError::from)?;

    if let Ok(pipeline) = runtime.capture_pipeline.lock() {
        pipeline.set_enabled(updated.capture_enabled);
    }

    Ok(AppSettingsDto::from(updated))
}
