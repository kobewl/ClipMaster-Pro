//! 敏感内容策略：发往模型之前的本地第一道门。
//!
//! 这是「宁可多拦」的门，而不是完整 DLP。规则都是有界的正则（长度下限 + 左边界），
//! 避免旧实现里 `text.contains("sk-")` 那种把 `desk-top`、`task-list` 也当成密钥的误拦。
//! 判断过程刻意不让内容进日志。
//!
//! 左边界用显式字符类 `(?:^|[^A-Za-z0-9_-])` 而不是 `\b`：regex crate 的 `\b`
//! 按 Unicode 算，「是sk-xxx」这种中文紧贴的写法在 `\b` 下不算词边界，会漏掉。
//!
//! `sk-` 规则额外要求主体里至少有一位数字，用来排除 `sk-primary-button-container-large`
//! 这类 kebab-case 的 CSS 类名和 slug。regex crate 不支持前瞻，「长度 ≥16」与「含数字」
//! 写不进同一个 pattern，所以这条规则不在 [`RULES`] 里，而是单独匹配后再检查。

use std::sync::OnceLock;

use regex::{Regex, RegexSet};

/// `sk-` 前缀密钥（OpenAI / DeepSeek / Anthropic 等）。捕获组 1 是 `sk-` 之后的主体。
const OPENAI_KEY: &str = r"(?:^|[^A-Za-z0-9_-])sk-([A-Za-z0-9_-]{16,})";

/// （规则名，pattern）。规则名只用于测试定位，不进日志。
const RULES: &[(&str, &str)] = &[
    (
        "github_token",
        r"(?:^|[^A-Za-z0-9_])(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})",
    ),
    (
        "google_api_key",
        r"(?:^|[^A-Za-z0-9_-])AIza[0-9A-Za-z_-]{35}",
    ),
    // 要求带数字段，避免把 `xoxo-gossip` 这类普通词误判成 token。
    (
        "slack_token",
        r"(?:^|[^A-Za-z0-9_])xox[abposr]-[0-9]{6,}-[0-9A-Za-z-]{6,}",
    ),
    (
        "aws_access_key",
        r"(?:^|[^A-Za-z0-9])AKIA[0-9A-Z]{16}(?:[^A-Za-z0-9]|$)",
    ),
    (
        "bearer_token",
        r"(?i)(?:^|[^a-z0-9_])bearer\s+[A-Za-z0-9._~+/-]{16,}=*",
    ),
    (
        "pem_private_key",
        r"(?i)-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----",
    ),
    (
        "en_secret_label",
        r"(?i)(?:^|[^a-z0-9_])(?:password|passwd|api[_-]?key|secret[_-]?key|access[_-]?token|client[_-]?secret)\s*[:=]\s*\S",
    ),
    ("zh_secret_label", r"(?:密码|口令|密钥|秘钥)\s*[:：=]\s*\S"),
];

fn rule_set() -> &'static RegexSet {
    static SET: OnceLock<RegexSet> = OnceLock::new();
    SET.get_or_init(|| {
        RegexSet::new(RULES.iter().map(|(_, pattern)| *pattern)).expect("敏感规则必须能编译")
    })
}

fn openai_key_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(OPENAI_KEY).expect("sk- 规则必须能编译"))
}

/// 是否含 `sk-` 开头、主体 ≥16 位且至少带一位数字的片段。
fn looks_like_openai_key(text: &str) -> bool {
    openai_key_regex()
        .captures_iter(text)
        .any(|caps| caps[1].bytes().any(|b| b.is_ascii_digit()))
}

/// 文本里是否含疑似密钥 / 凭据。命中任一规则或任一空白分隔的片段形似 JWT 即为 true。
pub(crate) fn contains_sensitive_content(text: &str) -> bool {
    rule_set().is_match(text)
        || looks_like_openai_key(text)
        || text.split_whitespace().any(looks_like_jwt)
}

fn looks_like_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(a) = parts.next() else { return false };
    let Some(b) = parts.next() else { return false };
    let Some(c) = parts.next() else { return false };
    parts.next().is_none()
        && a.len() >= 10
        && b.len() >= 10
        && c.len() >= 10
        && [a, b, c].into_iter().all(|part| {
            part.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // fixture 一律运行时拼接，不写形似真密钥的字面量（避免 secret scanning 误报）。

    fn hits(rule: &str, text: &str) -> bool {
        let index = RULES
            .iter()
            .position(|(name, _)| *name == rule)
            .unwrap_or_else(|| panic!("未知规则 {rule}"));
        rule_set().matches(text).matched(index)
    }

    #[test]
    fn blocks_common_secrets_without_logging_them() {
        assert!(contains_sensitive_content("-----BEGIN PRIVATE KEY-----"));
        assert!(contains_sensitive_content(&format!(
            "token=sk-{}",
            "x1".repeat(10)
        )));
        assert!(contains_sensitive_content(
            "aaaabbbbbb.cccccccccc.dddddddddd"
        ));
        assert!(!contains_sensitive_content(
            "Redis timeout happened after deployment."
        ));
    }

    #[test]
    fn openai_key_rule() {
        let positives = [
            format!("key sk-{}", "a1".repeat(10)),
            format!("sk-proj-{}", "A1_".repeat(8)),
            format!("sk-proj-{}", "Ab3_".repeat(6)),
            // DeepSeek 形状：sk- 加 32 位十六进制
            format!("sk-{}", "0a".repeat(16)),
            format!("sk-ant-api03-{}", "Zz9-".repeat(5)),
        ];
        for text in &positives {
            assert!(looks_like_openai_key(text), "{text}");
            assert!(contains_sensitive_content(text), "{text}");
        }
        assert!(
            looks_like_openai_key(&format!("我的key是sk-{}", "x1".repeat(10))),
            "中文紧贴也要命中"
        );

        let negatives = [
            "desk-top".to_string(),
            "task-list".to_string(),
            "sk-short".to_string(),
            "use desk-topographic".to_string(),
            // kebab-case 的 CSS 类名 / slug：长度够但主体没有数字
            "sk-primary-button-container-large".to_string(),
            "sk-fading-circle-animation-wrapper".to_string(),
            format!("sk-{}", "x".repeat(24)),
        ];
        for text in &negatives {
            assert!(!looks_like_openai_key(text), "{text}");
            assert!(!contains_sensitive_content(text), "{text}");
        }
        assert!(
            !looks_like_openai_key(&format!("task-{}", "x1".repeat(10))),
            "前面紧贴字母不算 sk- 开头"
        );
    }

    #[test]
    fn github_token_rule() {
        assert!(hits("github_token", &format!("ghp_{}", "x".repeat(36))));
        assert!(hits(
            "github_token",
            &format!("token: gho_{}", "Y".repeat(40))
        ));
        assert!(hits(
            "github_token",
            &format!("github_pat_{}", "a_b".repeat(10))
        ));
        assert!(!hits("github_token", "ghp_short"));
        assert!(!hits("github_token", "github_pat_tiny"));
        assert!(!contains_sensitive_content("ghp_short"));
    }

    #[test]
    fn google_api_key_rule() {
        assert!(hits("google_api_key", &format!("AIza{}", "B".repeat(35))));
        assert!(!hits("google_api_key", "AIzaShort"));
        assert!(!contains_sensitive_content("AIzaShort"));
    }

    #[test]
    fn slack_token_rule() {
        assert!(hits(
            "slack_token",
            &format!("xoxb-{}-{}-abc", "1".repeat(12), "2".repeat(12))
        ));
        assert!(hits(
            "slack_token",
            &format!("xoxp-{}-{}", "9".repeat(8), "z".repeat(8))
        ));
        assert!(!hits("slack_token", "xoxo-gossip-girl"));
        assert!(!hits("slack_token", "xoxb-abc"));
        assert!(!contains_sensitive_content("xoxo-gossip-girl"));
    }

    #[test]
    fn aws_access_key_rule() {
        assert!(hits("aws_access_key", &format!("AKIA{}", "Q".repeat(16))));
        assert!(hits(
            "aws_access_key",
            &format!("id=AKIA{}\n", "0".repeat(16))
        ));
        assert!(!hits("aws_access_key", &format!("AKIA{}", "Q".repeat(10))));
        assert!(
            !hits("aws_access_key", &format!("AKIA{}", "Q".repeat(20))),
            "过长不是 AWS key 的形状"
        );
        assert!(!contains_sensitive_content("AKIA is a prefix"));
    }

    #[test]
    fn bearer_token_rule() {
        assert!(hits(
            "bearer_token",
            &format!("Authorization: Bearer {}", "t".repeat(24))
        ));
        assert!(hits(
            "bearer_token",
            &format!("bearer {}==", "a.b".repeat(8))
        ));
        assert!(!hits("bearer_token", "Bearer of bad news"));
        assert!(!hits("bearer_token", "the bearer bond"));
        assert!(!contains_sensitive_content("Bearer of bad news"));
    }

    #[test]
    fn pem_private_key_rule() {
        assert!(hits("pem_private_key", "-----BEGIN RSA PRIVATE KEY-----"));
        assert!(hits(
            "pem_private_key",
            "-----BEGIN OPENSSH PRIVATE KEY-----"
        ));
        assert!(hits(
            "pem_private_key",
            "-----BEGIN PGP PRIVATE KEY BLOCK-----"
        ));
        assert!(!hits("pem_private_key", "-----BEGIN PUBLIC KEY-----"));
        assert!(!hits("pem_private_key", "-----BEGIN CERTIFICATE-----"));
        assert!(!contains_sensitive_content("-----BEGIN PUBLIC KEY-----"));
    }

    #[test]
    fn en_secret_label_rule() {
        assert!(hits("en_secret_label", "password=hunter2"));
        assert!(hits("en_secret_label", "Password: hunter2"));
        assert!(hits("en_secret_label", "API_KEY = abc"));
        assert!(hits("en_secret_label", "client-secret:xyz"));
        assert!(!hits("en_secret_label", "password reset link"));
        assert!(
            !hits("en_secret_label", "mypassword=1"),
            "左边界：不是独立的词"
        );
        assert!(!hits("en_secret_label", "password:"), "冒号后没有值");
        assert!(!contains_sensitive_content("password reset link"));
    }

    #[test]
    fn zh_secret_label_rule() {
        assert!(hits("zh_secret_label", "密码：abc123"));
        assert!(hits("zh_secret_label", "WiFi 密码: guest"));
        assert!(hits("zh_secret_label", "口令=open"));
        assert!(hits("zh_secret_label", "秘钥：xyz"));
        assert!(!hits("zh_secret_label", "忘记密码怎么办"));
        assert!(!hits("zh_secret_label", "密码学"));
        assert!(!contains_sensitive_content("忘记密码怎么办"));
        assert!(!contains_sensitive_content("密码学入门"));
    }

    #[test]
    fn jwt_shape_is_still_detected() {
        assert!(contains_sensitive_content(&format!(
            "{}.{}.{}",
            "a".repeat(12),
            "b".repeat(12),
            "c".repeat(12)
        )));
        assert!(!contains_sensitive_content("version 1.2.3 released"));
    }
}
