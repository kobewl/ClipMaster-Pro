//! Repository 集成测试。覆盖 CRUD、去重、分组、清理边界、FTS5 trigram。

use std::sync::{Arc, Mutex};

use app_lib::domain::model::{ClipboardItemId, ContentType, NewClipboardItem};
use app_lib::domain::normalize::compute_fingerprint;
use app_lib::domain::ports::{ClipboardRepository, GroupRepository, SearchQuery};
use app_lib::infrastructure::sqlite::group_repository::SqliteGroupRepository;
use app_lib::infrastructure::sqlite::repository::SqliteClipboardRepository;

fn new_repo_with_conn() -> (SqliteClipboardRepository, SqliteGroupRepository, Arc<Mutex<rusqlite::Connection>>) {
    let mut conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    app_lib::infrastructure::sqlite::migrations::run_migrations(&mut conn).expect("run migrations");
    let conn = Arc::new(Mutex::new(conn));
    (
        SqliteClipboardRepository::new(conn.clone()),
        SqliteGroupRepository::new(conn.clone()),
        conn,
    )
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
        source_url: None,
    }
}

#[tokio::test]
async fn insert_then_search_returns_item() {
    let repo = new_repo();
    let item = repo.insert_or_touch(new_text_item("hello world")).await.expect("insert");
    let result = repo.search(SearchQuery { group_id: None, search_text: None, limit: 10, offset: 0 }).await.expect("search");
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].id, item.id);
}

#[tokio::test]
async fn duplicate_dedup_preserves_group() {
    let (repo, grepo, _) = new_repo_with_conn();
    let first = repo.insert_or_touch(new_text_item("dup")).await.unwrap();
    let group = grepo.create_group("Test".into(), "#FF0000".into()).await.unwrap();
    repo.set_group(first.id, Some(group.id.clone())).await.unwrap();

    let second = repo.insert_or_touch(new_text_item("dup")).await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.group_id.as_deref(), Some(group.id.as_str()), "去重后分组状态必须保留");
}

#[tokio::test]
async fn search_by_group_id() {
    let (repo, grepo, _) = new_repo_with_conn();
    let g = grepo.create_group("Code".into(), "#3B82F6".into()).await.unwrap();
    let a = repo.insert_or_touch(new_text_item("in group")).await.unwrap();
    let _b = repo.insert_or_touch(new_text_item("not in group")).await.unwrap();
    repo.set_group(a.id, Some(g.id.clone())).await.unwrap();

    let result = repo.search(SearchQuery { group_id: Some(g.id), search_text: None, limit: 10, offset: 0 }).await.unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].id, a.id);
}

#[tokio::test]
async fn fts_trigram_chinese_search() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("ClipMaster 剪贴板管理工具")).await.unwrap();
    for (q, expect, desc) in [
        ("剪贴板", 1, "3字中文"), ("管理工", 1, "3字中文中间"),
        ("Clip", 1, "英文前缀"), ("master", 1, "英文不区分大小写"),
        ("xyz", 0, "不存在"),
    ] {
        let r = repo.search(SearchQuery { group_id: None, search_text: Some(q.to_string()), limit: 10, offset: 0 }).await.unwrap();
        assert_eq!(r.total, expect, "[{desc}] 搜 \"{q}\"");
    }
}

#[tokio::test]
async fn enforce_max_count_protects_grouped() {
    let (repo, grepo, _) = new_repo_with_conn();
    let g = grepo.create_group("Pin".into(), "#F00".into()).await.unwrap();
    let pinned = repo.insert_or_touch(new_text_item("keep")).await.unwrap();
    repo.set_group(pinned.id, Some(g.id)).await.unwrap();
    for i in 0..5 { repo.insert_or_touch(new_text_item(&format!("item {i}"))).await.unwrap(); }

    let c = repo.enforce_max_count(2).await.unwrap();
    assert_eq!(c.deleted_count, 3);
    let r = repo.search(SearchQuery { limit: 100, ..Default::default() }).await.unwrap();
    assert!(r.items.iter().any(|i| i.id == pinned.id), "分组条目不得被清理");
}

#[tokio::test]
async fn clear_keep_grouped() {
    let (repo, grepo, _) = new_repo_with_conn();
    let g = grepo.create_group("Pin".into(), "#F00".into()).await.unwrap();
    let pinned = repo.insert_or_touch(new_text_item("fav")).await.unwrap();
    repo.set_group(pinned.id, Some(g.id)).await.unwrap();
    repo.insert_or_touch(new_text_item("not fav")).await.unwrap();

    let c = repo.clear(true).await.unwrap();
    assert_eq!(c.deleted_count, 1);
    let r = repo.search(SearchQuery { limit: 100, ..Default::default() }).await.unwrap();
    assert_eq!(r.total, 1);
    assert_eq!(r.items[0].id, pinned.id);
}

#[tokio::test]
async fn delete_nonexistent() {
    let repo = new_repo();
    let r = repo.delete(ClipboardItemId::new()).await.unwrap();
    assert!(!r.deleted);
}

#[tokio::test]
async fn retention_zero_deletes_nothing() {
    let repo = new_repo();
    repo.insert_or_touch(new_text_item("old")).await.unwrap();
    let c = repo.enforce_retention_days(0).await.unwrap();
    assert_eq!(c.deleted_count, 0);
}

#[tokio::test]
async fn retention_protects_grouped() {
    let (repo, grepo, conn) = new_repo_with_conn();
    let g = grepo.create_group("Keep".into(), "#0F0".into()).await.unwrap();
    let old_item = repo.insert_or_touch(new_text_item("old")).await.unwrap();
    let old_grouped = repo.insert_or_touch(new_text_item("old grouped")).await.unwrap();
    repo.set_group(old_grouped.id, Some(g.id)).await.unwrap();
    let _fresh = repo.insert_or_touch(new_text_item("fresh")).await.unwrap();

    {
        let conn = conn.lock().unwrap();
        let old_ts = (chrono::Utc::now() - chrono::Duration::days(40)).to_rfc3339();
        conn.execute(
            "UPDATE clipboard_items SET last_copied_at = ?1 WHERE id IN (?2, ?3)",
            rusqlite::params![old_ts, old_item.id.to_string(), old_grouped.id.to_string()],
        ).unwrap();
    }

    let c = repo.enforce_retention_days(30).await.unwrap();
    assert_eq!(c.deleted_count, 1);
    let r = repo.search(SearchQuery { limit: 100, ..Default::default() }).await.unwrap();
    assert!(r.items.iter().any(|i| i.id == old_grouped.id), "过期但分组的条目受保护");
}

// Group CRUD
#[tokio::test]
async fn group_crud() {
    let (_, grepo, _) = new_repo_with_conn();
    let g = grepo.create_group("Test".into(), "#FF0000".into()).await.unwrap();
    assert_eq!(g.name, "Test");
    let groups = grepo.list_groups().await.unwrap();
    assert_eq!(groups.len(), 1);
    grepo.update_group(g.id.clone(), "Renamed".into(), "#00FF00".into()).await.unwrap();
    grepo.delete_group(g.id).await.unwrap();
    assert!(grepo.list_groups().await.unwrap().is_empty());
}

// Source info round-trip
#[tokio::test]
async fn source_info_preserved() {
    let repo = new_repo();
    let item = repo.insert_or_touch(NewClipboardItem {
        content_type: ContentType::Text,
        content_text: "from chrome".into(),
        fingerprint: compute_fingerprint("text", "from chrome"),
        source_app: Some("Google Chrome".into()),
        source_url: Some("https://github.com".into()),
    }).await.unwrap();
    assert_eq!(item.source_app.as_deref(), Some("Google Chrome"));
    assert_eq!(item.source_url.as_deref(), Some("https://github.com"));
}
