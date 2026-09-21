//! 应用设置的领域模型。
//! 字段对齐数据文档与产品需求文档 FR-SET-*。

use serde::{Deserialize, Serialize};

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
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            max_history: 1000,
            retention_days: 30,
            capture_enabled: true,
            shortcut: DEFAULT_SHORTCUT.to_string(),
            theme: THEME_SYSTEM.to_string(),
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
        Ok(())
    }
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
        "Cmd", "Ctrl", "CmdOrCtrl", "Shift", "Alt", "Option", "Super",
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
        let s = AppSettings { shortcut: String::new(), ..Default::default() };
        assert!(s.validate().is_ok(), "空字符串表示不绑定快捷键");
    }

    #[test]
    fn single_key_without_modifier_is_rejected() {
        let s = AppSettings { shortcut: "V".to_string(), ..Default::default() };
        assert!(s.validate().is_err());
    }

    #[test]
    fn valid_shortcut_passes() {
        let s = AppSettings { shortcut: "Alt+Shift+C".to_string(), ..Default::default() };
        assert!(s.validate().is_ok());
    }

    #[test]
    fn theme_must_be_one_of_three() {
        for theme in [THEME_SYSTEM, THEME_LIGHT, THEME_DARK] {
            let s = AppSettings { theme: theme.to_string(), ..Default::default() };
            assert!(s.validate().is_ok(), "theme={theme}");
        }
        let bad = AppSettings { theme: "auto".to_string(), ..Default::default() };
        assert!(bad.validate().is_err());
    }
}
