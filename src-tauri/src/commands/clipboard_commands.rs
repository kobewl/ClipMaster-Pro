use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::dto::{
    AppSettingsDto, ClipGroupDto, ClipboardItemDto, CreateGroupDto, ListQueryDto, ListResultDto,
    UpdateGroupDto,
};
use crate::domain::error::CommandError;
use crate::lifecycle::runtime::AppRuntime;
use crate::lifecycle::shortcut::swap_shortcut;

fn emit_or_warn<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!(error = %err, event, "发送事件失败");
    }
}

const MAX_PAGE_SIZE: u32 = 500;

// ---------------------------------------------------------------------------
//  Clipboard CRUD
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_clipboard_items(
    runtime: State<'_, AppRuntime>,
    query: ListQueryDto,
) -> Result<ListResultDto, CommandError> {
    let limit = query.limit.clamp(1, MAX_PAGE_SIZE);
    let search_query = crate::application::history_service::HistoryService::build_search_query(
        query.group_id,
        query.search,
        limit,
        query.offset,
    );
    let result = runtime.history.list(search_query).await.map_err(CommandError::from)?;
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
    emit_or_warn(&app, "clipboard://deleted", &id);
    Ok(())
}

#[tauri::command]
pub async fn set_item_group(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
    group_id: Option<String>,
) -> Result<(), CommandError> {
    runtime.history.set_group(&id, group_id).await.map_err(CommandError::from)?;
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
    runtime.history.copy_to_clipboard(&id).await.map_err(CommandError::from)?;
    if let Ok(item) = runtime.history.get(&id).await {
        emit_or_warn(&app, "clipboard://updated", ClipboardItemDto::from(item));
    }
    Ok(())
}

#[tauri::command]
pub async fn paste_clipboard_item(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<(), CommandError> {
    runtime.history.copy_to_clipboard(&id).await.map_err(CommandError::from)?;
    if let Ok(item) = runtime.history.get(&id).await {
        emit_or_warn(&app, "clipboard://updated", ClipboardItemDto::from(item));
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    #[cfg(target_os = "macos")]
    crate::infrastructure::clipboard::macos::simulate_paste();
    Ok(())
}

#[tauri::command]
pub async fn clear_history(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    keep_grouped: bool,
) -> Result<u64, CommandError> {
    let deleted = runtime.history.clear(keep_grouped).await.map_err(CommandError::from)?;
    emit_or_warn(&app, "clipboard://cleared", deleted);
    Ok(deleted)
}

// ---------------------------------------------------------------------------
//  Group CRUD
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_groups(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<ClipGroupDto>, CommandError> {
    let groups = runtime.groups.list().await.map_err(CommandError::from)?;
    Ok(groups.into_iter().map(ClipGroupDto::from).collect())
}

#[tauri::command]
pub async fn create_group(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    dto: CreateGroupDto,
) -> Result<ClipGroupDto, CommandError> {
    let group = runtime.groups.create(dto.name, dto.color).await.map_err(CommandError::from)?;
    let dto = ClipGroupDto::from(group);
    emit_or_warn(&app, "groups://changed", "created");
    Ok(dto)
}

#[tauri::command]
pub async fn update_group(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    dto: UpdateGroupDto,
) -> Result<ClipGroupDto, CommandError> {
    let group = runtime.groups.update(dto.id, dto.name, dto.color).await.map_err(CommandError::from)?;
    let dto = ClipGroupDto::from(group);
    emit_or_warn(&app, "groups://changed", "updated");
    Ok(dto)
}

#[tauri::command]
pub async fn delete_group(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<(), CommandError> {
    runtime.groups.delete(id).await.map_err(CommandError::from)?;
    emit_or_warn(&app, "groups://changed", "deleted");
    Ok(())
}

// ---------------------------------------------------------------------------
//  Settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_settings(
    runtime: State<'_, AppRuntime>,
) -> Result<AppSettingsDto, CommandError> {
    runtime.settings.get().await.map(AppSettingsDto::from).map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_settings(
    runtime: State<'_, AppRuntime>,
    settings: AppSettingsDto,
) -> Result<AppSettingsDto, CommandError> {
    let updated = runtime.settings.update(settings.into()).await.map_err(CommandError::from)?;
    if let Ok(pipeline) = runtime.capture_pipeline.lock() {
        pipeline.set_enabled(updated.capture_enabled);
    }
    Ok(AppSettingsDto::from(updated))
}

#[tauri::command]
pub async fn update_shortcut(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    shortcut: String,
) -> Result<AppSettingsDto, CommandError> {
    let old_settings = runtime.settings.get().await.map_err(CommandError::from)?;
    swap_shortcut(&app, &old_settings.shortcut, &shortcut).map_err(CommandError::shortcut)?;
    let mut new_settings = old_settings;
    new_settings.shortcut = shortcut;
    let saved = runtime.settings.update(new_settings).await.map_err(CommandError::from)?;
    Ok(AppSettingsDto::from(saved))
}

#[tauri::command]
pub async fn set_capture_enabled(
    runtime: State<'_, AppRuntime>,
    enabled: bool,
) -> Result<AppSettingsDto, CommandError> {
    let updated = runtime.settings.set_capture_enabled(enabled).await.map_err(CommandError::from)?;
    if let Ok(pipeline) = runtime.capture_pipeline.lock() {
        pipeline.set_enabled(updated.capture_enabled);
    }
    Ok(AppSettingsDto::from(updated))
}
