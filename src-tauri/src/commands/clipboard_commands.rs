use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::dto::{
    AppSettingsDto, ClipGroupDto, ClipboardItemDto, ClipboardItemIdPayload, CreateGroupDto,
    ListQueryDto, ListResultDto, SourceAppDto, SourceIconRequestDto, UpdateGroupDto,
};
use crate::domain::error::CommandError;
use crate::domain::model::ContentType;
use crate::lifecycle::runtime::AppRuntime;
use crate::lifecycle::shortcut::swap_shortcut;

fn emit_or_warn<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!(error = %err, event, "发送事件失败");
    }
}

const MAX_PAGE_SIZE: u32 = 500;

/// 把前端的预设时间档换算成筛选起点（RFC3339 / UTC，与 last_copied_at 列的存储
/// 格式一致 —— 字符串比较即时间比较）。None = 不限时间。
///
/// 「今天」按本地时区的零点算，换算完**必须转回 UTC** 再序列化：否则
/// `+08:00` 和 `+00:00` 两种后缀混在一起，字典序就不再是时间序。
fn time_range_to_since(range: Option<&str>) -> Option<String> {
    match range? {
        "today" => {
            let midnight_local = chrono::Local::now().date_naive().and_hms_opt(0, 0, 0)?;
            let midnight_utc = midnight_local
                .and_local_timezone(chrono::Local)
                .single()?
                .with_timezone(&chrono::Utc);
            Some(midnight_utc.to_rfc3339())
        }
        "week" => Some((chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339()),
        "month" => Some((chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
//  Clipboard CRUD
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_clipboard_items(
    runtime: State<'_, AppRuntime>,
    query: ListQueryDto,
) -> Result<ListResultDto, CommandError> {
    let limit = query.limit.clamp(1, MAX_PAGE_SIZE);
    // 非法的类型字符串 → 稳定错误码 invalid_content_type（透传到前端显示）。
    let content_type = query
        .content_type
        .as_deref()
        .map(|s| s.parse::<ContentType>())
        .transpose()
        .map_err(|err| CommandError::from(crate::domain::error::AppError::Domain(err)))?;
    let search_query = crate::application::history_service::HistoryService::build_search_query(
        query.group_id,
        query.search,
        content_type,
        time_range_to_since(query.time_range.as_deref()),
        limit,
        query.offset,
    );
    let result = runtime
        .history
        .list(search_query)
        .await
        .map_err(CommandError::from)?;
    // 命中词按条目 id 分发到各行：装配在应用层完成，这里只做 DTO 层的搬运。
    let mut matched_by_id = result.matched_terms.unwrap_or_default();
    let items = result
        .items
        .into_iter()
        .map(|item| {
            let mut dto = ClipboardItemDto::from(item);
            dto.matched_terms = matched_by_id.remove(&dto.id);
            dto
        })
        .collect();
    Ok(ListResultDto {
        items,
        total: result.total,
        relaxed_dropped: result.relaxed_dropped,
    })
}

#[tauri::command]
pub async fn delete_clipboard_item(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<(), CommandError> {
    runtime
        .history
        .delete(&id)
        .await
        .map_err(CommandError::from)?;
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
    runtime
        .history
        .set_group(&id, group_id)
        .await
        .map_err(CommandError::from)?;
    // 只发 id：前端回查列表，不必为了发事件再把全文读一遍。
    emit_or_warn(&app, "clipboard://updated", ClipboardItemIdPayload { id });
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
    emit_or_warn(&app, "clipboard://updated", ClipboardItemIdPayload { id });
    Ok(())
}

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
    emit_or_warn(&app, "clipboard://updated", ClipboardItemIdPayload { id });
    hide_window_and_simulate_paste(&app).await
}

/// 把一段文本写进剪贴板（多选合并复制用），不写入历史记录。
#[tauri::command]
pub async fn copy_text_to_clipboard(
    runtime: State<'_, AppRuntime>,
    text: String,
) -> Result<(), CommandError> {
    runtime
        .history
        .copy_text_to_clipboard(&text)
        .await
        .map_err(CommandError::from)
}

/// 合并复制之后直接粘贴：写剪贴板 → 隐藏窗口 → 模拟 ⌘V。
#[tauri::command]
pub async fn paste_text(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    text: String,
) -> Result<(), CommandError> {
    runtime
        .history
        .copy_text_to_clipboard(&text)
        .await
        .map_err(CommandError::from)?;
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

/// 解析一批来源应用的真实图标。返回值与 `sources` **按位置一一对应**：
/// 第 i 项是第 i 个来源的 PNG 绝对路径，`null` = 找不到，前端退回 emoji。
///
/// 每个来源按「复制时记下的 `.app` 路径 → bundle id → 应用名」的顺序找应用
/// （见 `appicon::resolve_app_path`）。第 1 步只是 stat，在阻塞线程池里做
/// （`appicon::stored_app_path`）；只有它没找到的才在主线程走后两步
/// （`appicon::lookup_app_path`）。后台检查超时时，主线程会跳过这批已记录路径
/// （系统给出的路径恰好是它们时既不 stat 也不渲染），那几条这次退回 emoji。
/// 缓存文件按 bundle id（旧记录按应用名）命名，已经在缓存里的直接读盘，
/// 不会再走一次 AppKit。
#[tauri::command]
pub async fn get_source_icons(
    app: AppHandle,
    sources: Vec<SourceIconRequestDto>,
) -> Result<Vec<Option<String>>, CommandError> {
    let mut result: Vec<Option<String>> = vec![None; sources.len()];

    #[cfg(target_os = "macos")]
    {
        use crate::infrastructure::appicon::{self, IconSource};

        let Some(cache_dir) = appicon::cache_dir(&app) else {
            tracing::warn!("无法创建图标缓存目录，来源图标退回 emoji");
            return Ok(result);
        };

        let mut missing: Vec<(usize, String, IconSource)> = Vec::new();
        for (index, request) in sources.into_iter().enumerate() {
            let source = IconSource::from(request);
            let Some(file_name) = appicon::icon_cache_file_name(&source) else {
                continue;
            };
            let cached = cache_dir.join(&file_name);
            if cached.is_file() {
                result[index] = Some(cached.to_string_lossy().to_string());
            } else {
                missing.push((index, file_name, source));
            }
        }
        if missing.is_empty() {
            return Ok(result);
        }

        // 第 1 步（复制时记下的 `.app` 路径是否还在）只是一次文件系统 stat，
        // 不需要 AppKit。那个路径可能在网络卷上，stat 会卡 —— 放到阻塞线程池里做，
        // 不占主线程。超时或任务异常时降级：全部当作「没有可用的已记录路径」，
        // 交给主线程按 bundle id / 应用名去找（顺序不变）；同时把这些已记录路径
        // 全部交给主线程跳过 —— 后台没在时限内答完，哪一条都可能在那个慢卷上。
        let fallback: Vec<(usize, String, IconSource, Option<String>)> = missing
            .iter()
            .cloned()
            .map(|(index, file_name, source)| (index, file_name, source, None))
            .collect();
        let slow_paths: Vec<String> = missing
            .iter()
            .filter_map(|(_, _, source)| source.app_path.as_deref())
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect();
        let (pre_resolved, avoid) = match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::task::spawn_blocking(move || {
                missing
                    .into_iter()
                    .map(|(index, file_name, source)| {
                        let stored =
                            appicon::stored_app_path(&source, &appicon::macos::MacAppLocator);
                        (index, file_name, source, stored)
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .await
        {
            Ok(Ok(resolved)) => (resolved, Vec::new()),
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "检查已记录的应用路径失败，改按 bundle id / 应用名查找，并跳过已记录路径");
                (fallback, slow_paths)
            }
            Err(_) => {
                tracing::warn!(
                    "检查已记录的应用路径超时，改按 bundle id / 应用名查找，主线程跳过这些慢路径"
                );
                (fallback, slow_paths)
            }
        };

        // AppKit 只在主线程用：整批一次性丢过去，省得每个应用来回一趟。
        // bundle id / 运行中的应用找路径都要碰 NSWorkspace，渲染也必须在主线程。
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            let rendered: Vec<(usize, String, Vec<u8>)> = pre_resolved
                .into_iter()
                .filter_map(|(index, file_name, source, stored)| {
                    let bytes = stored
                        .or_else(|| {
                            appicon::lookup_app_path(
                                &source,
                                &appicon::macos::MacAppLocator,
                                &avoid,
                            )
                        })
                        .and_then(|path| appicon::macos::render_icon_png_at(&path));
                    if bytes.is_none() {
                        tracing::debug!(file = %file_name, "来源应用图标未找到，退回 emoji");
                    }
                    bytes.map(|bytes| (index, file_name, bytes))
                })
                .collect();
            let _ = tx.send(rendered);
        })
        .map_err(|err| CommandError::icon(format!("调度图标渲染失败: {err}")))?;

        // 主线程万一没跑到这个闭包，也不能把命令挂死在这里。
        match tokio::time::timeout(std::time::Duration::from_secs(3), rx).await {
            Ok(Ok(rendered)) => {
                // 系统图标原图有 1~2MB，落盘时会用 sips 缩到 128px（约 10KB）。
                // sips 是同步子进程，整批放进一次阻塞线程池任务，不占 async worker。
                let save_dir = cache_dir.clone();
                let saved = tokio::task::spawn_blocking(move || {
                    let mut saved: Vec<(usize, String)> = Vec::new();
                    for (index, file_name, bytes) in rendered {
                        let path = save_dir.join(&file_name);
                        match appicon::macos::save_icon_png(&path, &bytes) {
                            Ok(()) => saved.push((index, path.to_string_lossy().to_string())),
                            Err(err) => {
                                tracing::warn!(error = %err, file = %file_name, "写入图标缓存失败")
                            }
                        }
                    }
                    saved
                })
                .await;
                match saved {
                    Ok(saved) => {
                        for (index, path) in saved {
                            result[index] = Some(path);
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "写入图标缓存任务异常，这一批退回 emoji")
                    }
                }
            }
            Ok(Err(_)) => tracing::warn!("图标渲染结果通道被提前关闭"),
            Err(_) => tracing::warn!("渲染应用图标超时，本次跳过"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = sources;

    Ok(result)
}

#[tauri::command]
pub async fn clear_history(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    keep_grouped: bool,
) -> Result<u64, CommandError> {
    let deleted = runtime
        .history
        .clear(keep_grouped)
        .await
        .map_err(CommandError::from)?;
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
    let group = runtime
        .groups
        .create(dto.name, dto.color)
        .await
        .map_err(CommandError::from)?;
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
    let group = runtime
        .groups
        .update(dto.id, dto.name, dto.color)
        .await
        .map_err(CommandError::from)?;
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
    runtime
        .groups
        .delete(id)
        .await
        .map_err(CommandError::from)?;
    emit_or_warn(&app, "groups://changed", "deleted");
    Ok(())
}

// ---------------------------------------------------------------------------
//  Settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_settings(runtime: State<'_, AppRuntime>) -> Result<AppSettingsDto, CommandError> {
    runtime
        .settings
        .get()
        .await
        .map(AppSettingsDto::from)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    runtime: State<'_, AppRuntime>,
    settings: AppSettingsDto,
) -> Result<AppSettingsDto, CommandError> {
    let updated = runtime
        .settings
        .update(settings.into())
        .await
        .map_err(CommandError::from)?;
    // 保存的设置里也可能带着新的采集开关 —— 与其它入口走同一条联动，
    // 否则只在这里改开关时，前端状态栏和托盘菜单都不会刷新。
    crate::lifecycle::runtime::sync_capture_side_effects(&app, &runtime, updated.capture_enabled);
    runtime
        .capture_gate
        .set_excluded_apps(updated.excluded_apps.clone());
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
    let saved = runtime
        .settings
        .update(new_settings)
        .await
        .map_err(CommandError::from)?;
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

/// 「不记录这些应用」名单：立即生效（落库后马上刷新采集闸门），不等「保存」。
#[tauri::command]
pub async fn set_excluded_apps(
    runtime: State<'_, AppRuntime>,
    apps: Vec<String>,
) -> Result<AppSettingsDto, CommandError> {
    let updated = runtime
        .set_excluded_apps(apps)
        .await
        .map_err(CommandError::from)?;
    Ok(AppSettingsDto::from(updated))
}

/// 最近复制过内容的来源应用（去重，最近的在前），供设置面板挑选加入排除名单。
#[tauri::command]
pub async fn list_recent_source_apps(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<SourceAppDto>, CommandError> {
    let apps = runtime
        .history
        .recent_source_apps(30)
        .await
        .map_err(CommandError::from)?;
    Ok(apps.into_iter().map(SourceAppDto::from).collect())
}
