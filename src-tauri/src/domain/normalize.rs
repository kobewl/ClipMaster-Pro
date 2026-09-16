//! 文本标准化与 fingerprint 计算。
//!
//! 决策依据（数据文档第 4 节，待决策事项 D-003）：
//! - 保留原文首尾空白和内部换行，不在存储层擅自“修正”用户内容。
//! - fingerprint 计算前做 Unicode NFC 正规化 + 统一换行符，
//!   避免同一段内容因为来源应用/系统差异（CRLF vs LF、组合字符）产生不同哈希。
//! - v2 的 `content_hash` 仅对原始字节做 MD5，未做任何标准化；
//!   v3 在此基础上加入正规化是有意为之的改进，出现在 ADR 中而非静默变更。
//!
//! 算法选择：SHA-256 而非 v2 的 MD5，冲突概率更低，且不用于加密安全场景，
//! 纯粹作为去重键（架构文档“不把哈希当作加密保护”的提示）。

use sha2::{Digest, Sha256};

/// 统一换行符为 `\n`，不删除或折叠内部空白。
fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

/// 计算用于去重的 fingerprint。
/// 输入：内容类型 + 换行统一后的 NFC 正规化文本（保留原始大小写和首尾空白）。
pub fn compute_fingerprint(content_type: &str, content: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    let normalized_newlines = normalize_newlines(content);
    let nfc: String = normalized_newlines.nfc().collect();

    let mut hasher = Sha256::new();
    hasher.update(content_type.as_bytes());
    hasher.update(b"\0");
    hasher.update(nfc.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}

/// 生成用于本地搜索的规范化文本：大小写不敏感匹配的基础（FR-SEA-001）。
pub fn build_search_text(content: &str) -> String {
    content.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_content_different_newlines_share_fingerprint() {
        let a = compute_fingerprint("text", "hello\r\nworld");
        let b = compute_fingerprint("text", "hello\nworld");
        assert_eq!(a, b);
    }

    #[test]
    fn different_content_has_different_fingerprint() {
        let a = compute_fingerprint("text", "hello");
        let b = compute_fingerprint("text", "hello ");
        assert_ne!(a, b, "首尾空白差异必须产生不同 fingerprint，不做静默裁剪");
    }

    #[test]
    fn search_text_is_case_insensitive_ready() {
        assert_eq!(build_search_text("HeLLo"), "hello");
    }
}
