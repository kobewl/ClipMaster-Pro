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
    hide_window_and_simulate_paste(&app).await
}

/// 把一段文本写进剪贴板（多选合并复制用），不写入历史记录。
#[tauri::command]
pub async fn copy_text_to_clipboard(
    runtime: State<'_, AppRuntime>,
    text: String,
) -> Result<(), CommandError> {
    runtime.history.copy_text_to_clipboard(&text).await.map_err(CommandError::from)
}

/// 合并复制之后直接粘贴：写剪贴板 → 隐藏窗口 → 模拟 ⌘V。
#[tauri::command]
pub async fn paste_text(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    text: String,
) -> Result<(), CommandError> {
    runtime.history.copy_text_to_clipboard(&text).await.map_err(CommandError::from)?;
    hide_window_and_simulate_paste(&app).await
}

/// 隐藏主窗口，等焦点回到用户原来的应用，再投一次 ⌘V。
///
/// 「一键粘贴」和「合并后粘贴」共用同一段收尾逻辑，两处必须完全一致：
/// 少等那 150ms，事件就会打在一个正在失去焦点的窗口上，表现为「点了没反应」。
async fn hide_window_and_simulate_paste(app: &AppHandle) -> Result<(), CommandError> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // osascript 是同步阻塞的（几百毫秒），扔到阻塞线程池，别占着 async worker。
    #[cfg(target_os = "macos")]
    tokio::task::spawn_blocking(crate::infrastructure::appicon::macos::simulate_paste)
        .await
        .map_err(|err| CommandError::paste(format!("模拟粘贴任务异常: {err}")))?
        .map_err(CommandError::paste)?;

    Ok(())
}

/// 解析一批来源应用的真实图标，返回 `{应用名: PNG 绝对路径}`。
///
/// 前端拿路径用 `convertFileSrc` 显示；系统里找不到的应用**不会**出现在结果里，
/// 由前端退回 emoji 兜底。已经在缓存里的应用直接读盘，不会再走一次 AppKit。
#[tauri::command]
pub async fn get_source_icons(
    app: AppHandle,
    apps: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, CommandError> {
    let mut result = std::collections::HashMap::new();

    #[cfg(target_os = "macos")]
    {
        use crate::infrastructure::appicon;

        let Some(cache_dir) = appicon::cache_dir(&app) else {
            tracing::warn!("无法创建图标缓存目录，来源图标退回 emoji");
            return Ok(result);
        };

        let mut missing: Vec<String> = Vec::new();
        for name in apps {
            let cached = cache_dir.join(appicon::cache_file_name(&name));
            if cached.is_file() {
                result.insert(name, cached.to_string_lossy().to_string());
            } else {
                missing.push(name);
            }
        }
        if missing.is_empty() {
            return Ok(result);
        }

        // AppKit 只在主线程用：整批一次性丢过去，省得每个应用来回一趟。
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            let rendered: Vec<(String, Vec<u8>)> = missing
                .into_iter()
                .filter_map(|name| {
                    appicon::macos::render_app_icon_png(&name).map(|bytes| (name, bytes))
                })
                .collect();
            let _ = tx.send(rendered);
        })
        .map_err(|err| CommandError::icon(format!("调度图标渲染失败: {err}")))?;

        // 主线程万一没跑到这个闭包，也不能把命令挂死在这里。
        match tokio::time::timeout(std::time::Duration::from_secs(3), rx).await {
            Ok(Ok(rendered)) => {
                for (name, bytes) in rendered {
                    let path = cache_dir.join(appicon::cache_file_name(&name));
                    // 系统图标原图有 1~2MB，落盘时会用 sips 缩到 128px（约 10KB）。
                    match appicon::macos::save_icon_png(&path, &bytes) {
                        Ok(()) => {
                            result.insert(name, path.to_string_lossy().to_string());
                        }
                        Err(err) => tracing::warn!(error = %err, app = %name, "写入图标缓存失败"),
                    }
                }
            }
            Ok(Err(_)) => tracing::warn!("图标渲染结果通道被提前关闭"),
            Err(_) => tracing::warn!("渲染应用图标超时，本次跳过"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = apps;

    Ok(result)
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
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    settings: AppSettingsDto,
) -> Result<AppSettingsDto, CommandError> {
    let updated = runtime.settings.update(settings.into()).await.map_err(CommandError::from)?;
    // 保存的设置里也可能带着新的采集开关 —— 与其它入口走同一条联动，
    // 否则只在这里改开关时，前端状态栏和托盘菜单都不会刷新。
    crate::lifecycle::runtime::sync_capture_side_effects(&app, &runtime, updated.capture_enabled);
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
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    enabled: bool,
) -> Result<AppSettingsDto, CommandError> {
    // 唯一写入路径在 AppRuntime::set_capture_enabled：设置面板、状态栏、托盘
    // 三个入口共用，pipeline 投影 / 前端事件 / 托盘文案在这里统一联动。
    let updated = runtime
        .set_capture_enabled(&app, enabled)
        .await
        .map_err(CommandError::from)?;
    Ok(AppSettingsDto::from(updated))
}
