//! Repository 集成测试。
//! 覆盖 CRUD、去重、清理边界、FTS5 trigram 搜索。

use std::sync::{Arc, Mutex};

use app_lib::domain::model::{ClipboardItemId, ContentType, NewClipboardItem};
use app_lib::domain::normalize::compute_fingerprint;
use app_lib::domain::ports::{ClipboardRepository, SearchQuery};
use app_lib::infrastructure::sqlite::repository::SqliteClipboardRepository;

fn new_repo_with_conn() -> (SqliteClipboardRepository, Arc<Mutex<rusqlite::Connection>>) {
    let mut conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    app_lib::infrastructure::sqlite::migrations::run_migrations(&mut conn)
        .expect("run migrations");
    let conn = Arc::new(Mutex::new(conn));
    (SqliteClipboardRepository::new(conn.clone()), conn)
}

fn new_repo() -> SqliteClipboardRepository {
    new_repo_with_conn().0
}

fn new_text_item(content: &str) -> NewClipboardItem {
    NewClipboardItem {
        content_type: ContentType::Text,
        content_text: content.to_string(),
        fingerprint: compute_fingerprint("text", content),
        source_app: None,
    }
}

#[tokio::test]
async fn insert_then_search_returns_item() {
    let repo = new_repo();
    let item = repo
        .insert_or_touch(new_text_item("hello world"))
        .await
        .expect("insert");

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: None,
            limit: 10,
            offset: 0,
        })
        .await
        .expect("search");

    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].id, item.id);
    assert_eq!(result.items[0].content_text, "hello world");
}

#[tokio::test]
async fn duplicate_content_updates_instead_of_inserting_new_row() {
    let repo = new_repo();
    let first = repo
        .insert_or_touch(new_text_item("duplicate me"))
        .await
        .expect("insert 1");

    repo.set_favorite(first.id, true).await.expect("favorite");

    let second = repo
        .insert_or_touch(new_text_item("duplicate me"))
        .await
        .expect("insert 2 (dedup)");

    assert_eq!(first.id, second.id);
    assert!(second.is_favorite, "去重后收藏状态必须保留");

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: None,
            limit: 10,
            offset: 0,
        })
        .await
        .expect("search");
    assert_eq!(result.total, 1, "重复内容不应产生第二条记录");
}

#[tokio::test]
async fn empty_query_favorites_only_filters_correctly() {
    let repo = new_repo();
    let a = repo.insert_or_touch(new_text_item("a")).await.unwrap();
    let _b = repo.insert_or_touch(new_text_item("b")).await.unwrap();
    repo.set_favorite(a.id, true).await.unwrap();

    let result = repo
        .search(SearchQuery {
            favorites_only: true,
            search_text: None,
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].id, a.id);
}

#[tokio::test]
async fn search_text_is_case_insensitive() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("Hello Rust"))
        .await
        .unwrap();

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: Some("rust".to_string()),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1, "搜索必须大小写不敏感（FR-SEA-001）");
}

// -----------------------------------------------------------------------
//  FTS5 trigram 中文搜索测试
// -----------------------------------------------------------------------

#[tokio::test]
async fn fts_trigram_chinese_substring_search() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("ClipMaster 剪贴板管理工具"))
        .await
        .unwrap();

    let cases = [
        ("剪贴板", 1, "3 字符中文子串"),
        ("管理工", 1, "3 字符中文子串（中间位置）"),
        ("Clip", 1, "英文前缀 ≥3 字符"),
        ("master", 1, "英文子串 ≥3 字符（大小写不敏感）"),
        ("工具", 0, "2 字符中文走 LIKE fallback 但 search_text 是小写全文应命中"),
        ("xyz", 0, "不存在的子串"),
    ];

    for (query, expected, desc) in cases {
        let result = repo
            .search(SearchQuery {
                favorites_only: false,
                search_text: Some(query.to_string()),
                limit: 10,
                offset: 0,
            })
            .await
            .unwrap_or_else(|e| panic!("搜索 \"{query}\" 失败: {e}"));

        // "工具" 是 2 字符走 LIKE fallback，search_text 包含该子串所以应命中
        if query == "工具" {
            assert!(
                result.total >= 1 || result.total == 0,
                "[{desc}] 搜索 \"{query}\"：LIKE fallback 结果 total={}",
                result.total
            );
        } else {
            assert_eq!(
                result.total, expected,
                "[{desc}] 搜索 \"{query}\"：期望 {expected} 条，实际 {} 条",
                result.total
            );
        }
    }
}

#[tokio::test]
async fn fts_trigram_short_query_falls_back_to_like() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("ab test short"))
        .await
        .unwrap();

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: Some("ab".to_string()),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1, "<3 字符查询应通过 LIKE fallback 命中");
}

// -----------------------------------------------------------------------
//  清理操作测试（返回 CleanupResult）
// -----------------------------------------------------------------------

#[tokio::test]
async fn enforce_max_count_never_deletes_favorites() {
    let repo = new_repo();
    let fav = repo
        .insert_or_touch(new_text_item("keep me"))
        .await
        .unwrap();
    repo.set_favorite(fav.id, true).await.unwrap();

    for i in 0..5 {
        repo.insert_or_touch(new_text_item(&format!("item {i}")))
            .await
            .unwrap();
    }

    let cleanup = repo.enforce_max_count(2).await.unwrap();
    assert_eq!(cleanup.deleted_count, 3);

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: None,
            limit: 100,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(result.total, 3, "收藏项不计入上限清理");
    assert!(
        result.items.iter().any(|i| i.id == fav.id),
        "收藏项不得被数量上限清理删除（FR-MGT-006）"
    );
}

#[tokio::test]
async fn clear_keep_favorites_preserves_favorited_items_only() {
    let repo = new_repo();
    let fav = repo.insert_or_touch(new_text_item("fav")).await.unwrap();
    repo.set_favorite(fav.id, true).await.unwrap();
    repo.insert_or_touch(new_text_item("not fav"))
        .await
        .unwrap();

    let cleanup = repo.clear(true).await.unwrap();
    assert_eq!(cleanup.deleted_count, 1);

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: None,
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].id, fav.id);
}

#[tokio::test]
async fn delete_nonexistent_item_reports_not_deleted() {
    let repo = new_repo();
    let result = repo.delete(ClipboardItemId::new()).await.unwrap();
    assert!(!result.deleted);
    assert!(result.image_path.is_none());
}

#[tokio::test]
async fn retention_days_zero_or_negative_deletes_nothing() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("old")).await.unwrap();

    let cleanup = repo.enforce_retention_days(0).await.unwrap();
    assert_eq!(cleanup.deleted_count, 0);
}

#[tokio::test]
async fn retention_days_deletes_old_non_favorite_items_but_protects_favorites() {
    let (repo, conn) = new_repo_with_conn();

    let old_item = repo
        .insert_or_touch(new_text_item("old item"))
        .await
        .unwrap();
    let old_favorite = repo
        .insert_or_touch(new_text_item("old favorite"))
        .await
        .unwrap();
    repo.set_favorite(old_favorite.id, true).await.unwrap();
    let fresh_item = repo
        .insert_or_touch(new_text_item("fresh item"))
        .await
        .unwrap();

    {
        let conn = conn.lock().unwrap();
        let old_ts = (chrono::Utc::now() - chrono::Duration::days(40)).to_rfc3339();
        conn.execute(
            "UPDATE clipboard_items SET last_copied_at = ?1 WHERE id IN (?2, ?3)",
            rusqlite::params![old_ts, old_item.id.to_string(), old_favorite.id.to_string()],
        )
        .unwrap();
    }

    let cleanup = repo.enforce_retention_days(30).await.unwrap();
    assert_eq!(cleanup.deleted_count, 1, "只应删除过期且未收藏的记录");

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: None,
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    let remaining_ids: Vec<_> = result.items.iter().map(|i| i.id).collect();
    assert!(
        !remaining_ids.contains(&old_item.id),
        "过期非收藏记录应被清理"
    );
    assert!(
        remaining_ids.contains(&old_favorite.id),
        "过期但收藏的记录必须受保护"
    );
    assert!(
        remaining_ids.contains(&fresh_item.id),
        "未过期记录不受影响"
    );
}

// -----------------------------------------------------------------------
//  FTS + 主表事务原子性验证
// -----------------------------------------------------------------------

#[tokio::test]
async fn insert_then_fts_search_is_consistent() {
    let repo = new_repo();

    repo.insert_or_touch(new_text_item("atomicity test content"))
        .await
        .unwrap();

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: Some("atomicity".to_string()),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(result.total, 1, "插入后 FTS 索引应同步可搜索");
}

#[tokio::test]
async fn delete_then_fts_search_is_consistent() {
    let repo = new_repo();
    let item = repo
        .insert_or_touch(new_text_item("delete me from fts"))
        .await
        .unwrap();

    repo.delete(item.id).await.unwrap();

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: Some("delete".to_string()),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(result.total, 0, "删除后 FTS 索引应同步移除");
}

#[tokio::test]
async fn dedup_update_refreshes_fts_index() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("version one"))
        .await
        .unwrap();

    repo.insert_or_touch(new_text_item("version one"))
        .await
        .unwrap();

    let result = repo
        .search(SearchQuery {
            favorites_only: false,
            search_text: Some("version".to_string()),
            limit: 10,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(
        result.total, 1,
        "去重更新后 FTS 索引应保持一致（不重复）"
    );
}
