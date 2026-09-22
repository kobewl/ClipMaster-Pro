//! 注入防护：转义 `clip` 标签、标记疑似指令文本、剥掉属性值里的引号。
//!
//! 内容来自网页、聊天记录等外部来源，里面可能写着 "忽略之前的指令"。

/// 把内容里出现的 clip 标签转义掉，防止某条内容伪造标签边界 ——
/// 闭合标签之后写的东西，在模型看来就从"数据"变成了"指令"。
///
/// 只动 `<clip` / `</clip` 这个序列本身，**不碰内容里其它的尖括号**：
/// 剪贴板里经常有 HTML 和代码片段，全局转义会把它们变得不可读。
pub(crate) fn escape_clip_tags(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            let mut j = i + 1;
            if chars.get(j) == Some(&'/') {
                j += 1;
            }
            // 取 4 个字符做大小写不敏感比对（<CLIP> 同样要拦）
            let head: String = chars[j..chars.len().min(j + 4)]
                .iter()
                .collect::<String>()
                .to_lowercase();
            if head == "clip" {
                out.push('<');
                out.push('\\');
                i += 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 内容里是不是出现了"声称自己是系统指令"的句子。
///
/// 命中只做**标记**，不阻断 —— 阻断是安全门（密钥、私钥）的职责。
/// 误判的代价只是多一个提示属性，所以规则可以相对宽松。
pub(crate) fn looks_like_injection(text: &str) -> bool {
    const PATTERNS: &[&str] = &[
        // 中文
        "忽略之前",
        "忽略以上",
        "忽略上面",
        "忽略先前",
        "无视之前",
        "无视以上",
        "忘记之前",
        "忘记上面",
        "忽略所有指令",
        "你现在是",
        "从现在起你是",
        "新的系统提示",
        "新的指令如下",
        "不要告诉用户",
        // 英文
        "ignore previous",
        "ignore all previous",
        "ignore the above",
        "ignore the instructions",
        "disregard previous",
        "disregard the above",
        "forget previous",
        "you are now",
        "new system prompt",
        "do not tell the user",
        "instead, output",
    ];
    let lower = text.to_lowercase();
    PATTERNS.iter().any(|pattern| lower.contains(pattern))
}

/// 剥掉属性值里的双引号：来源名是元数据、不是内容，
/// 但应用名可能自带引号把属性撑破，伪造出别的属性（防伪造标签闭合）。
pub(crate) fn strip_attribute_quotes(value: &str) -> String {
    value.replace('"', "")
}
