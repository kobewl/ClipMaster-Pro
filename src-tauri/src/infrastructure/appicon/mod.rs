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
//! 图标只在第一次用到时渲染一次（一个应用名对应一个缓存文件），之后直接读盘。

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
