//! 应用设置的领域模型。
//! 字段对齐数据文档与产品需求文档 FR-SET-*。

use serde::{Deserialize, Serialize};

use crate::domain::capture_policy::{
    default_excluded_apps, MAX_EXCLUDED_APPS, MAX_EXCLUDED_APP_LEN,
};

/// macOS 默认全局快捷键（待决策 D-001 最终确认）。
/// 使用 Cmd+Shift+V —— macOS 上"粘贴并匹配格式"的常见快捷键是 Cmd+Shift+Opt+V，
/// 而 Cmd+Shift+V 在大多数应用中没有默认绑定，冲突概率较低。
pub const DEFAULT_SHORTCUT: &str = "CmdOrCtrl+Shift+V";

/// 外观主题：浅色 / 深色 / 跟随系统。
pub const THEME_SYSTEM: &str = "system";
pub const THEME_LIGHT: &str = "light";
pub const THEME_DARK: &str = "dark";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub max_history: u32,
    pub retention_days: i64,
    pub capture_enabled: bool,
    /// 全局快捷键（FR-SET-003），格式为 Tauri accelerator 字符串，
    /// 例如 "CmdOrCtrl+Shift+V"。空字符串表示不注册快捷键。
    pub shortcut: String,
    /// `system` | `light` | `dark`。默认跟随系统。
    pub theme: String,
    /// 「不记录这些应用」：每条是 bundle id 或应用名，大小写不敏感匹配。
    /// 默认是钥匙串与常见密码管理器；用户清空后保持为空。
    pub excluded_apps: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            max_history: 1000,
            retention_days: 30,
            capture_enabled: true,
            shortcut: DEFAULT_SHORTCUT.to_string(),
            theme: THEME_SYSTEM.to_string(),
            excluded_apps: default_excluded_apps(),
        }
    }
}

impl AppSettings {
    /// 校验设置合法性；失败时保留原值不覆盖（FR-SET-006）。
    pub fn validate(&self) -> Result<(), String> {
        if self.max_history == 0 {
            return Err("最大历史数量必须大于 0".to_string());
        }
        if self.max_history > 200_000 {
            return Err("最大历史数量超出支持范围".to_string());
        }
        if self.retention_days < 0 {
            return Err("保留天数不能为负数".to_string());
        }
        if !self.shortcut.is_empty() {
            validate_shortcut_format(&self.shortcut)?;
        }
        validate_theme(&self.theme)?;
        validate_excluded_apps(&self.excluded_apps)?;
        Ok(())
    }
}

fn validate_excluded_apps(apps: &[String]) -> Result<(), String> {
    if apps.len() > MAX_EXCLUDED_APPS {
        return Err(format!("不记录的应用最多 {MAX_EXCLUDED_APPS} 个"));
    }
    for app in apps {
        if app.trim().is_empty() {
            return Err("不记录的应用名不能为空".to_string());
        }
        if app.chars().count() > MAX_EXCLUDED_APP_LEN {
            return Err(format!(
                "不记录的应用名过长（最多 {MAX_EXCLUDED_APP_LEN} 个字符）"
            ));
        }
    }
    Ok(())
}

fn validate_theme(theme: &str) -> Result<(), String> {
    match theme {
        THEME_SYSTEM | THEME_LIGHT | THEME_DARK => Ok(()),
        _ => Err(format!(
            "不支持的主题 \"{theme}\"，可用: system, light, dark"
        )),
    }
}

/// 校验快捷键字符串格式是否合法。
/// Tauri 使用 accelerator 格式，修饰键: Cmd/Ctrl/CmdOrCtrl/Shift/Alt/Option/Super，
/// 主键: A-Z、0-9、F1-F24、Space、Tab 等。
fn validate_shortcut_format(shortcut: &str) -> Result<(), String> {
    let parts: Vec<&str> = shortcut.split('+').collect();
    if parts.len() < 2 {
        return Err("快捷键至少需要一个修饰键和一个主键".to_string());
    }
    let key = parts.last().unwrap().trim();
    if key.is_empty() {
        return Err("快捷键主键不能为空".to_string());
    }
    let modifiers = &parts[..parts.len() - 1];
    if modifiers.is_empty() {
        return Err("快捷键至少需要一个修饰键（Cmd/Ctrl/Shift/Alt）".to_string());
    }
    let valid_modifiers = [
        "Cmd",
        "Ctrl",
        "CmdOrCtrl",
        "Shift",
        "Alt",
        "Option",
        "Super",
    ];
    for m in modifiers {
        let m = m.trim();
        if !valid_modifiers.iter().any(|v| v.eq_ignore_ascii_case(m)) {
            return Err(format!(
                "不支持的修饰键 \"{m}\"，可用: Cmd, Ctrl, CmdOrCtrl, Shift, Alt, Option"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shortcut_is_valid() {
        let settings = AppSettings::default();
        assert!(settings.validate().is_ok());
        assert_eq!(settings.theme, THEME_SYSTEM);
    }

    #[test]
    fn empty_shortcut_is_valid() {
        let s = AppSettings {
            shortcut: String::new(),
            ..Default::default()
        };
        assert!(s.validate().is_ok(), "空字符串表示不绑定快捷键");
    }

    #[test]
    fn single_key_without_modifier_is_rejected() {
        let s = AppSettings {
            shortcut: "V".to_string(),
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn valid_shortcut_passes() {
        let s = AppSettings {
            shortcut: "Alt+Shift+C".to_string(),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn theme_must_be_one_of_three() {
        for theme in [THEME_SYSTEM, THEME_LIGHT, THEME_DARK] {
            let s = AppSettings {
                theme: theme.to_string(),
                ..Default::default()
            };
            assert!(s.validate().is_ok(), "theme={theme}");
        }
        let bad = AppSettings {
            theme: "auto".to_string(),
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn default_excluded_apps_include_keychain() {
        let settings = AppSettings::default();
        assert!(settings
            .excluded_apps
            .iter()
            .any(|app| app == "com.apple.keychainaccess"));
    }

    #[test]
    fn excluded_apps_are_bounded() {
        let too_many = AppSettings {
            excluded_apps: (0..=MAX_EXCLUDED_APPS)
                .map(|i| format!("com.example.app{i}"))
                .collect(),
            ..Default::default()
        };
        assert!(too_many.validate().is_err());

        let too_long = AppSettings {
            excluded_apps: vec!["x".repeat(MAX_EXCLUDED_APP_LEN + 1)],
            ..Default::default()
        };
        assert!(too_long.validate().is_err());

        let blank = AppSettings {
            excluded_apps: vec!["  ".to_string()],
            ..Default::default()
        };
        assert!(blank.validate().is_err());

        let empty = AppSettings {
            excluded_apps: Vec::new(),
            ..Default::default()
        };
        assert!(empty.validate().is_ok(), "空名单是合法的");
    }
}
