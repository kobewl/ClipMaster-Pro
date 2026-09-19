//! 系统集成类命令：开机自启、检查更新。
//!
//! **自启状态不存进 settings 表。** 真正的开关是系统里的那个登录项文件
//! （macOS 上是 `~/Library/LaunchAgents/*.plist`），数据库里再存一份就有两个
//! 真相来源：用户在系统设置里改掉、或者系统清理了 plist，两边就对不上了。
//! 所以这里每次都直接问插件，界面上显示的一定是实际生效的状态。

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::domain::error::CommandError;
use crate::lifecycle::autostart::is_autostart_locked;

#[tauri::command]
pub async fn get_autostart_enabled(app: AppHandle) -> Result<bool, CommandError> {
    app.autolaunch()
        .is_enabled()
        .map_err(|err| CommandError::system(format!("读取开机自启状态失败: {err}")))
}

#[tauri::command]
pub async fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), CommandError> {
    if is_autostart_locked() {
        return Err(CommandError::system(
            "开发版不能设置开机自启：它需要开发服务器才能显示界面，注册成登录项后\
             开机只会弹出一个空白窗口。装好正式版本再来开这个开关。"
                .to_string(),
        ));
    }

    let manager = app.autolaunch();
    let result = if enabled { manager.enable() } else { manager.disable() };

    // 失败时把「想干什么」写进 message：只报一句系统错误，用户不知道是哪个开关没设上。
    result.map_err(|err| {
        let action = if enabled { "开启" } else { "关闭" };
        CommandError::system(format!("{action}开机自启失败: {err}"))
    })
}

// ---------------------------------------------------------------------------
//  检查更新
// ---------------------------------------------------------------------------

/// 更新检查指向的 GitHub 仓库（「检查更新」和设置面板的「前往下载」都用它）。
/// 仓库改名 / 换号时改这一行即可。
const GITHUB_REPO: &str = "kobewl/ClipMaster-Pro";

/// 检查更新的结果。`update_available = false` 时其余字段无意义。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateStatusDto {
    pub current_version: String,
    pub update_available: bool,
    pub latest_version: Option<String>,
    pub notes: Option<String>,
    pub release_url: Option<String>,
}

/// GitHub `releases/latest` 响应里我们关心的字段。
#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
    /// 发布说明（Markdown 原文），可能为空。
    #[serde(default)]
    body: String,
    /// release 页面地址，前端「前往下载」直接打开它。
    html_url: String,
}

/// 把 release tag 解析成 semver 版本（兼容 "v" 前缀，如 `v0.1.0`）。
///
/// 项目版本号（`0.1.0-beta.2`）和 release tag 都按 semver 比较，
/// 语义是现成的：`0.1.0`（正式版）> `0.1.0-beta.2`（预发布），
/// 预发布号按数字比较（`beta.10` > `beta.2`，不是字典序）。
fn parse_release_tag(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim_start_matches('v')).ok()
}

/// 判断 `latest_tag` 是否比当前运行版本新。
/// 任一侧解析不出 semver（打错了 tag）时保守返回 false —— 宁可漏报，不误导升级。
fn is_newer(current: &str, latest_tag: &str) -> bool {
    match (semver::Version::parse(current).ok(), parse_release_tag(latest_tag)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// 请求 GitHub API 的 `releases/latest`（当前仓库最新正式发布）。
async fn fetch_latest_release() -> Result<LatestRelease, CommandError> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        // GitHub API 强制要求 User-Agent，缺了直接 403。
        .header("User-Agent", "ClipMaster-Pro")
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|err| {
            CommandError::system(format!("无法连接 GitHub（请检查网络后重试）：{err}"))
        })?;

    // 404 = 仓库存在但还没发过任何 release，这是可解释的状态，不是故障。
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CommandError::new(
            "release_not_found",
            format!("仓库 {GITHUB_REPO} 还没有发布过版本（首次发布后即可正常检查）"),
            false,
        ));
    }
    if !resp.status().is_success() {
        return Err(CommandError::system(format!(
            "GitHub 返回了错误状态 {}",
            resp.status()
        )));
    }

    resp.json::<LatestRelease>()
        .await
        .map_err(|err| CommandError::system(format!("解析 GitHub 响应失败: {err}")))
}

/// 检查应用更新。
///
/// 做法学自 Tabularis（TabularisDB/tabularis）：**检查**和**安装**分开。
/// 检查直接请求 GitHub API 拿最新 release、自己比较版本号 —— 不需要
/// Apple 证书，也不需要 updater 插件的 minisign 密钥，只要 GitHub 上
/// 发了 release 这里就能查到。发现新版本后前端展示说明并打开 release
/// 页面下载。「应用内自动下载安装」是进阶项：届时才需要生成 minisign
/// 密钥对、配置 `plugins.updater` 并在 release 里附 latest.json，
/// 步骤见 `docs/release/RELEASE.md`。
#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateStatusDto, CommandError> {
    let current = app.package_info().version.to_string();
    let release = fetch_latest_release().await?;

    Ok(UpdateStatusDto {
        update_available: is_newer(&current, &release.tag_name),
        current_version: current,
        latest_version: Some(release.tag_name.trim_start_matches('v').to_string()),
        notes: (!release.body.is_empty()).then_some(release.body),
        release_url: Some(release.html_url),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_release_tag_accepts_v_prefix() {
        assert_eq!(parse_release_tag("v0.1.0"), Some(semver::Version::new(0, 1, 0)));
        assert_eq!(
            parse_release_tag("0.1.0"),
            Some(semver::Version::new(0, 1, 0))
        );
        assert_eq!(
            parse_release_tag("v0.2.0-beta.1"),
            Some(semver::Version::parse("0.2.0-beta.1").unwrap())
        );
    }

    #[test]
    fn parse_release_tag_rejects_garbage() {
        assert_eq!(parse_release_tag(""), None);
        assert_eq!(parse_release_tag("latest"), None);
        assert_eq!(parse_release_tag("v1"), None);
    }

    #[test]
    fn prerelease_is_older_than_release() {
        // 当前版本是 beta：同版本号的正式发布、任何补丁更新都算「有新版」。
        assert!(is_newer("0.1.0-beta.2", "v0.1.0"));
        assert!(is_newer("0.1.0-beta.2", "0.1.1"));
        assert!(is_newer("0.1.0-beta.2", "v0.2.0"));
    }

    #[test]
    fn prerelease_numbers_compare_numerically() {
        // beta.10 必须大于 beta.2（字典序会得出相反结论）。
        assert!(is_newer("0.1.0-beta.2", "v0.1.0-beta.10"));
        assert!(!is_newer("0.1.0-beta.10", "v0.1.0-beta.2"));
    }

    #[test]
    fn same_or_older_is_not_newer() {
        assert!(!is_newer("0.1.0", "v0.1.0"));
        assert!(!is_newer("0.1.0", "v0.0.9"));
        assert!(!is_newer("0.2.0", "v0.1.0"));
    }

    #[test]
    fn unparseable_tag_is_never_newer() {
        // 打错的 tag 宁可漏报，也不把用户引去一个错误的「升级」。
        assert!(!is_newer("0.1.0-beta.2", "not-a-version"));
        assert!(!is_newer("not-a-version", "v0.1.0"));
    }

    #[test]
    fn latest_release_parses_github_payload() {
        let payload = r##"{
            "tag_name": "v0.2.0",
            "body": "更新内容:\n- 修复若干问题",
            "html_url": "https://github.com/kobewl/ClipMaster-Pro/releases/tag/v0.2.0",
            "prerelease": false,
            "assets": []
        }"##;
        let release: LatestRelease = serde_json::from_str(payload).unwrap();
        assert_eq!(release.tag_name, "v0.2.0");
        assert!(is_newer("0.1.0-beta.2", &release.tag_name));
        assert_eq!(release.html_url, "https://github.com/kobewl/ClipMaster-Pro/releases/tag/v0.2.0");
    }

    #[test]
    fn update_status_dto_serializes_snake_case() {
        let dto = UpdateStatusDto {
            current_version: "0.1.0-beta.2".to_string(),
            update_available: true,
            latest_version: Some("0.2.0".to_string()),
            notes: Some("n".to_string()),
            release_url: Some("https://example.com".to_string()),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["current_version"], "0.1.0-beta.2");
        assert_eq!(json["update_available"], true);
        assert_eq!(json["latest_version"], "0.2.0");
        assert_eq!(json["release_url"], "https://example.com");
    }
}
