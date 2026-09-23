//! 对话 Agent 的只读工具：搜索历史、读取条目。
//!
//! 模型只能读，不能写剪贴板 / 分组 / 粘贴。写操作仍走用户确认卡片（后续阶段）。
//! 工具结果有条数和字数上限，敏感内容只回报「已拦截」，不把原文回灌模型。

use serde::Deserialize;
use serde_json::{json, Value};

use crate::application::agent_service::contains_sensitive_content;
use crate::application::history_service::HistoryService;
use crate::domain::model::{ContentType, ClipboardItem};
use crate::domain::normalize::strip_html_tags;
use crate::domain::ports::SearchQuery;

pub const SEARCH_LIMIT: u32 = 8;
pub const READ_LIMIT: usize = 5;
pub const PREVIEW_CHARS: usize = 220;
pub const READ_CHARS: usize = 2_000;

pub fn tool_schemas() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "search_clipboard",
                "description": "在用户的本地剪贴板历史里按关键词搜索。当用户提到某段内容、某次排查、某个应用里复制过的东西时使用。不要编造搜索结果。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "搜索词，空格分词。可以是自然语言问句，后台会去掉「请问 / 怎么」等虚词。"
                        },
                        "time_range": {
                            "type": "string",
                            "enum": ["today", "week", "month"],
                            "description": "可选时间范围。用户说「昨天 / 今天」用 today，「最近」用 week。"
                        }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_items",
                "description": "按 id 读取剪贴板正文，最多 5 条。先 search_clipboard 再读。命中敏感内容的条目不会返回正文。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "item_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "search_clipboard 返回的 id 列表"
                        }
                    },
                    "required": ["item_ids"]
                }
            }
        }
    ])
}

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default)]
    time_range: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadArgs {
    item_ids: Vec<String>,
}

pub async fn execute(
    history: &HistoryService,
    name: &str,
    arguments: &str,
) -> Result<String, String> {
    match name {
        "search_clipboard" => search_clipboard(history, arguments).await,
        "read_items" => read_items(history, arguments).await,
        _ => Ok(json!({ "error": "unknown_tool" }).to_string()),
    }
}

async fn search_clipboard(history: &HistoryService, arguments: &str) -> Result<String, String> {
    let args: SearchArgs = serde_json::from_str(arguments).map_err(|err| err.to_string())?;
    let query = args.query.trim();
    if query.is_empty() {
        return Ok(json!({ "items": [], "note": "empty_query" }).to_string());
    }
    let result = history
        .list(SearchQuery {
            search_text: Some(query.to_string()),
            since: time_range_to_since(args.time_range.as_deref()),
            limit: SEARCH_LIMIT,
            offset: 0,
            ..SearchQuery::default()
        })
        .await
        .map_err(|err| err.to_string())?;

    let items: Vec<Value> = result
        .items
        .into_iter()
        .take(SEARCH_LIMIT as usize)
        .map(|item| {
            let (preview, blocked) = search_preview(&item);
            json!({
                "id": item.id.to_string(),
                "source": item.source_app.unwrap_or_else(|| "未知来源".into()),
                "type": item.content_type.as_str(),
                "time": item.last_copied_at.format("%m-%d %H:%M").to_string(),
                "preview": preview,
                "blocked_sensitive": blocked,
            })
        })
        .collect();
    Ok(json!({ "total": result.total, "items": items }).to_string())
}

fn search_preview(item: &ClipboardItem) -> (String, bool) {
    match item.content_type {
        ContentType::Image => ("[图片]".into(), false),
        ContentType::Files => (take_chars(&item.content_text, PREVIEW_CHARS), false),
        ContentType::Text | ContentType::Html => {
            let text = if item.content_type == ContentType::Html {
                strip_html_tags(&item.content_text)
            } else {
                item.content_text.clone()
            };
            if contains_sensitive_content(&text) {
                ("[敏感内容已拦截]".into(), true)
            } else {
                (take_chars(&text, PREVIEW_CHARS), false)
            }
        }
    }
}

async fn read_items(history: &HistoryService, arguments: &str) -> Result<String, String> {
    let args: ReadArgs = serde_json::from_str(arguments).map_err(|err| err.to_string())?;
    let mut items = Vec::new();
    for (index, item_id) in args.item_ids.iter().take(READ_LIMIT).enumerate() {
        let item = match history.get(item_id).await {
            Ok(item) => item,
            Err(_) => {
                items.push(json!({
                    "index": index + 1,
                    "id": item_id,
                    "status": "missing"
                }));
                continue;
            }
        };
        let text = match item.content_type {
            ContentType::Text => item.content_text,
            ContentType::Html => strip_html_tags(&item.content_text),
            ContentType::Image | ContentType::Files => {
                items.push(json!({
                    "index": index + 1,
                    "id": item_id,
                    "status": "unsupported",
                    "type": item.content_type.as_str(),
                }));
                continue;
            }
        };
        if contains_sensitive_content(&text) {
            items.push(json!({
                "index": index + 1,
                "id": item_id,
                "status": "blocked_sensitive",
            }));
            continue;
        }
        items.push(json!({
            "index": index + 1,
            "id": item.id.to_string(),
            "source": item.source_app.unwrap_or_else(|| "未知来源".into()),
            "time": item.last_copied_at.format("%m-%d %H:%M").to_string(),
            "text": take_chars(&text, READ_CHARS),
        }));
    }
    Ok(json!({ "items": items }).to_string())
}

fn take_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let clipped: String = text.chars().take(max).collect();
    format!("{clipped}…")
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_chars_clips_on_unicode_boundaries() {
        assert_eq!(take_chars("你好世界", 2), "你好…");
        assert_eq!(take_chars("abc", 8), "abc");
    }

    #[test]
    fn search_preview_blocks_sensitive_text() {
        let item = ClipboardItem {
            id: crate::domain::model::ClipboardItemId::new(),
            content_type: ContentType::Text,
            content_text: "-----BEGIN PRIVATE KEY-----\nabc".into(),
            fingerprint: "fp".into(),
            group_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_copied_at: chrono::Utc::now(),
            source_app: None,
            source_url: None,
            legacy_id: None,
        };
        let (preview, blocked) = search_preview(&item);
        assert!(blocked, "{preview}");
        assert_eq!(preview, "[敏感内容已拦截]");
    }

    #[test]
    fn unknown_time_range_is_ignored() {
        assert!(time_range_to_since(Some("yesterday")).is_none());
        assert!(time_range_to_since(Some("week")).is_some());
    }
}
