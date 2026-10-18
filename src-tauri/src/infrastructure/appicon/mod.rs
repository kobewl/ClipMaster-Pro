//! 来源应用图标。
//!
//! 列表里每一行都要显示「这条内容是从哪个应用复制的」。只写应用名不够直观，
//! 所以这里去系统里取那个应用**真实的图标**，转成 PNG 缓存在本地，
//! 前端用 `convertFileSrc` 直接当图片显示。
//!
//! 为什么不用 emoji：emoji 是写死的映射表（`src/lib/sourceIcons.ts`），
//! 遇到表里没有的应用（比如 ZCode）只能退化成一个通用剪贴板图标，
//! 用户看不出内容来自哪里。真实图标不存在这个问题。
//!
//! 找 `.app` 的顺序（见 [`resolve_app_path`]）：复制时记下的 `.app` 路径（仍在
//! 磁盘上）→ 复制时记下的 bundle id（应用被挪过位置也能找到）→ 按应用名匹配
//! （只给没有 bundle 信息的旧记录兜底）。全部失败才由前端退回 emoji。
//!
//! 图标只在第一次用到时渲染一次（一个 bundle id —— 旧记录则是一个应用名 ——
//! 对应一个缓存文件），之后直接读盘。

#[cfg(target_os = "macos")]
pub mod macos;

use std::path::PathBuf;

/// 应用名 → 缓存文件名。
///
/// 用 SHA-256 前 32 个十六进制字符，而不是直接用应用名：应用名里可能带
/// `/`、`:` 这类文件系统不接受的字符，哈希之后既安全又稳定（同一个应用名
/// 永远落到同一个文件，不会重复渲染）。
pub fn cache_file_name(app_name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(app_name.as_bytes());
    format!("{}.png", &hex::encode(digest)[..32])
}

/// 要取图标的那个来源应用：复制时记下的应用名 / bundle id / `.app` 路径。
/// 旧记录只有 `name`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IconSource {
    pub name: Option<String>,
    pub bundle_id: Option<String>,
    pub app_path: Option<String>,
}

/// 「在系统里找应用」的三种手段。抽成 trait 是为了让 [`resolve_app_path`]
/// 的先后顺序不依赖 AppKit 就能测；macOS 实现见 `macos::MacAppLocator`。
pub trait AppLocator {
    /// 这个路径上是否真有一个应用包（应用被删 / 被挪走后返回 false）。
    fn bundle_exists(&self, path: &str) -> bool;
    /// 按 bundle id 问系统要 `.app` 路径（LaunchServices）。
    fn path_for_bundle_id(&self, bundle_id: &str) -> Option<String>;
    /// 按应用名找 `.app` 路径（运行中的应用 → 常见目录 → 别名表）。
    fn path_for_name(&self, name: &str) -> Option<String>;
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// 把来源解析成 `.app` 的绝对路径。后一种手段只在前一种失败时才会被调用。
///
/// 1. 复制时记下的路径：必须以 `.app` 结尾且仍在磁盘上。`.app` 检查防止前端
///    传来任意路径时去渲染一个普通文件 / 文件夹的图标。
/// 2. bundle id：应用被挪过位置（或复制时跑在 App Translocation 的临时目录里）
///    也能找到；系统给的路径同样要求仍在磁盘上，过期的登记记录直接跳过。
/// 3. 应用名：没有 bundle 信息的旧记录走这条（含别名表）。
pub fn resolve_app_path(source: &IconSource, locator: &impl AppLocator) -> Option<String> {
    stored_app_path(source, locator).or_else(|| lookup_app_path(source, locator, &[]))
}

/// 第 1 步：复制时记下的 `.app` 路径（仍在磁盘上才算）。
///
/// 只做一次文件系统 stat，不碰 AppKit —— 可以（也应该）在后台线程跑：
/// 记录的路径可能在网络卷上，stat 会卡，不能放在主线程。
pub fn stored_app_path(source: &IconSource, locator: &impl AppLocator) -> Option<String> {
    let path = non_empty(&source.app_path)?;
    if path.to_lowercase().ends_with(".app") && locator.bundle_exists(path) {
        return Some(path.to_string());
    }
    None
}

/// 第 2、3 步：先 bundle id，再应用名。要问 LaunchServices / NSWorkspace，
/// 在 macOS 上需要主线程。不会再去 stat 已记录的路径。
///
/// `avoid_paths`：后台检查超时的那些已记录路径（多半在网络卷等慢速位置）。
/// 主线程不能再碰它们 —— 系统给出的路径若正是其中之一，既不 stat 也不返回，
/// 这次退回 emoji，下次请求再试。
pub fn lookup_app_path(
    source: &IconSource,
    locator: &impl AppLocator,
    avoid_paths: &[String],
) -> Option<String> {
    if let Some(bundle_id) = non_empty(&source.bundle_id) {
        if let Some(path) = locator.path_for_bundle_id(bundle_id) {
            if !is_avoided(&path, avoid_paths) && locator.bundle_exists(&path) {
                return Some(path);
            }
        }
    }
    let name = non_empty(&source.name)?;
    locator
        .path_for_name(name)
        .filter(|path| !is_avoided(path, avoid_paths))
}

/// 路径是否在 `avoid` 里。两边都去掉首尾空白和末尾的 `/` 再比较。
fn is_avoided(path: &str, avoid: &[String]) -> bool {
    fn normalize(p: &str) -> &str {
        p.trim().trim_end_matches('/')
    }
    let path = normalize(path);
    avoid.iter().any(|entry| normalize(entry) == path)
}

/// 来源 → 图标缓存文件名。`None` = 什么标识都没有，不必去取。
///
/// 有 bundle id 时按 bundle id 记（加 `bundle:` 前缀，免得和某个显示名恰好长得
/// 像 bundle id 的应用撞名）：同一个应用不论显示名、安装位置怎么变都只缓存一份。
/// 没有 bundle id 的旧记录仍按应用名记，与之前的缓存文件是同一个，已有缓存继续命中。
pub fn icon_cache_file_name(source: &IconSource) -> Option<String> {
    if let Some(bundle_id) = non_empty(&source.bundle_id) {
        return Some(cache_file_name(&format!("bundle:{bundle_id}")));
    }
    non_empty(&source.name).map(cache_file_name)
}

/// 图标缓存目录：`$APPDATA/appicons`（与 `tauri.conf.json` 里 assetProtocol
/// 的 scope 保持一致，否则前端读不到这些文件）。
pub fn cache_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().ok()?.join("appicons");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    /// 记录每次调用的假 locator：既给结果，也证明后面的手段有没有被跳过。
    #[derive(Default)]
    struct FakeLocator {
        existing: HashSet<String>,
        by_bundle: HashMap<String, String>,
        by_name: HashMap<String, String>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeLocator {
        fn exists(mut self, path: &str) -> Self {
            self.existing.insert(path.into());
            self
        }
        fn bundle(mut self, id: &str, path: &str) -> Self {
            self.by_bundle.insert(id.into(), path.into());
            self
        }
        fn name(mut self, name: &str, path: &str) -> Self {
            self.by_name.insert(name.into(), path.into());
            self
        }
        fn called(&self, prefix: &str) -> bool {
            self.calls
                .borrow()
                .iter()
                .any(|call| call.starts_with(prefix))
        }
    }

    impl AppLocator for FakeLocator {
        fn bundle_exists(&self, path: &str) -> bool {
            self.calls.borrow_mut().push(format!("exists:{path}"));
            self.existing.contains(path)
        }
        fn path_for_bundle_id(&self, bundle_id: &str) -> Option<String> {
            self.calls.borrow_mut().push(format!("bundle:{bundle_id}"));
            self.by_bundle.get(bundle_id).cloned()
        }
        fn path_for_name(&self, name: &str) -> Option<String> {
            self.calls.borrow_mut().push(format!("name:{name}"));
            self.by_name.get(name).cloned()
        }
    }

    fn source(name: Option<&str>, bundle_id: Option<&str>, app_path: Option<&str>) -> IconSource {
        IconSource {
            name: name.map(str::to_string),
            bundle_id: bundle_id.map(str::to_string),
            app_path: app_path.map(str::to_string),
        }
    }

    #[test]
    fn stored_path_that_exists_is_used_first() {
        let locator = FakeLocator::default()
            .exists("/Applications/Foo.app")
            .exists("/Other/Foo.app")
            .bundle("com.foo", "/Other/Foo.app")
            .name("Foo", "/Other/Foo.app");
        let found = resolve_app_path(
            &source(Some("Foo"), Some("com.foo"), Some("/Applications/Foo.app")),
            &locator,
        );
        assert_eq!(found.as_deref(), Some("/Applications/Foo.app"));
        assert!(!locator.called("bundle:"), "路径可用时不该再问 bundle id");
        assert!(!locator.called("name:"), "路径可用时不该再按名字找");
    }

    #[test]
    fn missing_stored_path_falls_back_to_bundle_id() {
        let locator = FakeLocator::default()
            .exists("/Users/x/Apps/Foo.app")
            .bundle("com.foo", "/Users/x/Apps/Foo.app")
            .name("Foo", "/Elsewhere/Foo.app");
        let found = resolve_app_path(
            &source(Some("Foo"), Some("com.foo"), Some("/Applications/Foo.app")),
            &locator,
        );
        assert_eq!(found.as_deref(), Some("/Users/x/Apps/Foo.app"));
        assert!(!locator.called("name:"), "bundle id 找到了就不该再按名字找");
    }

    #[test]
    fn bundle_id_result_must_exist_on_disk() {
        let locator = FakeLocator::default()
            .exists("/Applications/Foo.app")
            .bundle("com.foo", "/Gone/Foo.app")
            .name("Foo", "/Applications/Foo.app");
        let found = resolve_app_path(&source(Some("Foo"), Some("com.foo"), None), &locator);
        assert_eq!(found.as_deref(), Some("/Applications/Foo.app"));
        assert!(locator.called("exists:/Gone/Foo.app"));
    }

    #[test]
    fn no_path_no_bundle_falls_back_to_name() {
        let locator = FakeLocator::default().name("idea", "/Applications/IntelliJ IDEA.app");
        let found = resolve_app_path(&source(Some("idea"), None, None), &locator);
        assert_eq!(found.as_deref(), Some("/Applications/IntelliJ IDEA.app"));
    }

    #[test]
    fn non_app_stored_path_is_ignored() {
        let locator = FakeLocator::default().exists("/etc/passwd");
        let found = resolve_app_path(&source(None, None, Some("/etc/passwd")), &locator);
        assert_eq!(found, None);
        assert!(
            !locator.called("exists:/etc/passwd"),
            "非 .app 路径连查都不该查"
        );
    }

    #[test]
    fn empty_strings_are_treated_as_missing() {
        let locator = FakeLocator::default();
        let found = resolve_app_path(&source(Some(""), Some(""), Some("")), &locator);
        assert_eq!(found, None);
        assert!(
            locator.calls.borrow().is_empty(),
            "{:?}",
            locator.calls.borrow()
        );
    }

    #[test]
    fn stored_app_path_only_checks_existence() {
        let locator = FakeLocator::default()
            .exists("/Volumes/Net/Foo.app")
            .bundle("com.foo", "/Other/Foo.app")
            .name("Foo", "/Other/Foo.app");
        let src = source(Some("Foo"), Some("com.foo"), Some("/Volumes/Net/Foo.app"));
        assert_eq!(
            stored_app_path(&src, &locator).as_deref(),
            Some("/Volumes/Net/Foo.app")
        );
        assert!(locator.called("exists:/Volumes/Net/Foo.app"));
        assert!(!locator.called("bundle:"), "第 1 步不该问 bundle id");
        assert!(!locator.called("name:"), "第 1 步不该按名字找");

        let gone = FakeLocator::default();
        assert_eq!(stored_app_path(&src, &gone), None);
        assert!(!gone.called("bundle:"));
        assert!(!gone.called("name:"));
    }

    #[test]
    fn lookup_app_path_never_stats_the_stored_path() {
        let locator = FakeLocator::default()
            .exists("/Users/x/Apps/Foo.app")
            .bundle("com.foo", "/Users/x/Apps/Foo.app");
        let src = source(Some("Foo"), Some("com.foo"), Some("/Volumes/Net/Foo.app"));
        assert_eq!(
            lookup_app_path(&src, &locator, &[]).as_deref(),
            Some("/Users/x/Apps/Foo.app")
        );
        assert!(
            !locator.called("exists:/Volumes/Net/Foo.app"),
            "已记录路径的检查归第 1 步（后台线程），这里不许再 stat"
        );
    }

    #[test]
    fn split_steps_compose_to_resolve_app_path() {
        let cases = [
            source(Some("Foo"), Some("com.foo"), Some("/Applications/Foo.app")),
            source(Some("Foo"), Some("com.foo"), Some("/Gone/Foo.app")),
            source(Some("Foo"), None, None),
            source(None, Some("com.foo"), None),
            source(None, None, Some("/etc/passwd")),
            IconSource::default(),
        ];
        for src in cases {
            let locator = || {
                FakeLocator::default()
                    .exists("/Applications/Foo.app")
                    .exists("/Users/x/Apps/Foo.app")
                    .bundle("com.foo", "/Users/x/Apps/Foo.app")
                    .name("Foo", "/Elsewhere/Foo.app")
            };
            let split = stored_app_path(&src, &locator())
                .or_else(|| lookup_app_path(&src, &locator(), &[]));
            assert_eq!(split, resolve_app_path(&src, &locator()), "{src:?}");
        }
    }

    #[test]
    fn lookup_skips_avoided_launch_services_path() {
        // 已记录路径的后台检查超时了；LaunchServices 又给出同一个慢路径 ——
        // 主线程不能去 stat 它，改按应用名找。
        for avoided in ["/Volumes/Net/Foo.app", "/Volumes/Net/Foo.app/"] {
            let locator = FakeLocator::default()
                .exists("/Volumes/Net/Foo.app")
                .bundle("com.foo", "/Volumes/Net/Foo.app")
                .name("Foo", "/Applications/Foo.app");
            let src = source(Some("Foo"), Some("com.foo"), Some("/Volumes/Net/Foo.app"));
            let found = lookup_app_path(&src, &locator, &[avoided.to_string()]);
            assert_eq!(found.as_deref(), Some("/Applications/Foo.app"), "{avoided}");
            assert!(
                !locator.called("exists:/Volumes/Net/Foo.app"),
                "avoid 里的路径主线程一次都不能碰：{avoided}"
            );
        }
    }

    #[test]
    fn lookup_returns_none_when_name_result_is_avoided() {
        let locator = FakeLocator::default().name("Foo", "/Volumes/Net/Foo.app");
        let src = source(Some("Foo"), None, Some("/Volumes/Net/Foo.app"));
        assert_eq!(
            lookup_app_path(&src, &locator, &[" /Volumes/Net/Foo.app ".to_string()]),
            None
        );
        // 不在 avoid 里时照常返回。
        assert_eq!(
            lookup_app_path(&src, &locator, &["/Volumes/Other.app".to_string()]).as_deref(),
            Some("/Volumes/Net/Foo.app")
        );
    }

    #[test]
    fn icon_cache_file_name_prefers_bundle_id() {
        let with_bundle =
            icon_cache_file_name(&source(Some("Safari"), Some("com.apple.Safari"), None));
        let name_only = icon_cache_file_name(&source(Some("Safari"), None, None));
        assert_ne!(with_bundle, name_only);
        // 旧记录的缓存文件名不变，已有缓存继续命中。
        assert_eq!(name_only, Some(cache_file_name("Safari")));
        // 同一个 bundle id，不论显示名 / 路径，只缓存一份。
        assert_eq!(
            with_bundle,
            icon_cache_file_name(&source(
                Some("Safari 浏览器"),
                Some("com.apple.Safari"),
                Some("/Applications/Safari.app"),
            ))
        );
        assert_eq!(icon_cache_file_name(&IconSource::default()), None);
    }

    #[test]
    fn cache_file_name_is_stable_and_path_safe() {
        // 同一个应用名永远映射到同一个文件（否则每次启动都要重新渲染一遍）
        assert_eq!(cache_file_name("Safari"), cache_file_name("Safari"));
        assert_ne!(cache_file_name("Safari"), cache_file_name("Chrome"));

        // 带斜杠 / 中文的应用名也不能拼出非法路径
        let weird = cache_file_name("某公司/内部:应用");
        assert!(!weird.contains('/'));
        assert!(!weird.contains(':'));
        assert!(weird.ends_with(".png"));
    }
}
