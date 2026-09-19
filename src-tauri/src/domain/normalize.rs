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

/// 对二进制数据（图片等）计算 fingerprint。
pub fn compute_fingerprint_bytes(content_type: &str, data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content_type.as_bytes());
    hasher.update(b"\0");
    hasher.update(data);
    let digest = hasher.finalize();
    hex::encode(digest)
}

/// 生成用于本地搜索的规范化文本：大小写不敏感匹配的基础（FR-SEA-001）。
pub fn build_search_text(content: &str) -> String {
    content.to_lowercase()
}

/// 去掉 HTML 标签，留下纯文本。
///
/// 用于富文本条目的**搜索**与**列表预览** —— 标签本身（`<div>`、`style=`）不是
/// 用户想搜也不想看的内容。只做单遍状态机扫描，不是完整的 HTML 解析：
/// 对搜索/预览这个用途，正确处理 `<`、`>` 的开合就够了，注释和 script 内容
/// 理论上会残留，但剪贴板里的 HTML 来自正文复制，这种脏数据可以接受。
///
/// **安全边界**：这个函数的输出**只用于** search_text 和纯文本预览，
/// 永远不用于把 HTML 渲染回页面 —— 展示侧的消毒由前端 DOMPurify 负责，
/// 两边职责不同，不能互相替代。
pub fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            ch if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
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

    #[test]
    fn strip_html_removes_tags_keeps_text() {
        let html = "<meta charset='utf-8'><p style=\"color:red\">剪贴板<b>管理</b></p>";
        assert_eq!(strip_html_tags(html), "剪贴板管理");
    }

    #[test]
    fn strip_html_survives_unclosed_tag() {
        // 现实里会有被截断的 HTML；不能因为一个没闭合的 < 就把整段丢掉。
        assert_eq!(strip_html_tags("正文一<b"), "正文一");
        assert_eq!(strip_html_tags("正文二<div>正文三"), "正文二正文三");
    }

    #[test]
    fn strip_html_plain_text_is_unchanged() {
        assert_eq!(strip_html_tags("没有任何标签"), "没有任何标签");
    }
}
