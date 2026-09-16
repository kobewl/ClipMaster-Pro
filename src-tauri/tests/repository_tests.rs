//! Repository 集成测试。
//! 覆盖测试与质量保障方案文档 2.3 节要求的 Repository CRUD、去重与清理边界。

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

    // 与 v2 的 delete+insert 行为不同：v3 保留同一个 id，并保留收藏状态（US-002/US-006）。
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

#[tokio::test]
async fn enforce_max_count_never_deletes_favorites() {
    let repo = new_repo();
    let fav = repo.insert_or_touch(new_text_item("keep me")).await.unwrap();
    repo.set_favorite(fav.id, true).await.unwrap();

    for i in 0..5 {
        repo.insert_or_touch(new_text_item(&format!("item {i}")))
            .await
            .unwrap();
    }

    // 上限设为 2：非收藏项共 5 条，应删除到只剩 2 条非收藏 + 1 条收藏。
    let deleted = repo.enforce_max_count(2).await.unwrap();
    assert_eq!(deleted, 3);

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
    repo.insert_or_touch(new_text_item("not fav")).await.unwrap();

    let deleted = repo.clear(true).await.unwrap();
    assert_eq!(deleted, 1);

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
}

#[tokio::test]
async fn retention_days_zero_or_negative_deletes_nothing() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("old")).await.unwrap();

    let deleted = repo.enforce_retention_days(0).await.unwrap();
    assert_eq!(deleted, 0);
}

#[tokio::test]
async fn retention_days_deletes_old_non_favorite_items_but_protects_favorites() {
    let (repo, conn) = new_repo_with_conn();

    let old_item = repo.insert_or_touch(new_text_item("old item")).await.unwrap();
    let old_favorite = repo
        .insert_or_touch(new_text_item("old favorite"))
        .await
        .unwrap();
    repo.set_favorite(old_favorite.id, true).await.unwrap();
    let fresh_item = repo.insert_or_touch(new_text_item("fresh item")).await.unwrap();

    // 直接把 old_item / old_favorite 的 last_copied_at 改到 40 天前，
    // 模拟“很久没有被重新复制过”的历史记录（保留天数以 last_copied_at 为准）。
    {
        let conn = conn.lock().unwrap();
        let old_ts = (chrono::Utc::now() - chrono::Duration::days(40)).to_rfc3339();
        conn.execute(
            "UPDATE clipboard_items SET last_copied_at = ?1 WHERE id IN (?2, ?3)",
            rusqlite::params![old_ts, old_item.id.to_string(), old_favorite.id.to_string()],
        )
        .unwrap();
    }

    // 保留 30 天：old_item 应被删除，old_favorite 因收藏受保护，fresh_item 未过期。
    let deleted = repo.enforce_retention_days(30).await.unwrap();
    assert_eq!(deleted, 1, "只应删除过期且未收藏的记录");

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
    assert!(!remaining_ids.contains(&old_item.id), "过期非收藏记录应被清理");
    assert!(
        remaining_ids.contains(&old_favorite.id),
        "过期但收藏的记录必须受保护，不得被按天清理删除"
    );
    assert!(remaining_ids.contains(&fresh_item.id), "未过期记录不受影响");
}
