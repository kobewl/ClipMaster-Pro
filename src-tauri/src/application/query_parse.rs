//! 查询解析：把用户敲进的「一句话」解析成用于检索的词集合。
//!
//! 定位是**纯函数**：不碰数据库、不读设置，规则全部写死在这里，
//! 由 `HistoryService::list` 调用。这样「放宽阈值」「停用词表」这类
//! 语义有唯一一处定义，单测也不需要真库。
//!
//! 设计口径（宁少勿多）：停用词表只收确定的纯虚词与检索前缀 ——
//! 多收一个词，就可能把用户真正想搜的内容词吃掉；少收的代价只是
//! 多带一个无意义词进检索（放宽阶段本来就会兜住它）。

/// 整词停用词：单独出现时不携带检索信息。
///
/// 刻意**不含**「做法」「目的」这类看似虚词、实际常是内容词的词 ——
/// 它们落在原文里的概率很高，剔掉等于自断召回。
const STOP_WORDS: &[&str] = &[
    "的", "了", "吗", "呢", "吧", "啊", "一下", "找", "找找", "搜索", "关于",
    "请问", "帮我", "如何", "怎么", "怎样", "为什么", "是什么", "怎么做",
];

/// 检索前缀：出现在词首时剥掉，剩下的部分才是内容。
///
/// 只剥前缀、不剥后缀：「目的」「为了」「我的」这类正常词以「的/了」结尾，
/// 按后缀剥会把它们切坏（宁少勿多）。表按长度从长到短排，保证
/// 「怎么做」不会被「怎么」先吃掉一半。
const QUERY_PREFIXES: &[&str] = &[
    "为什么", "是什么", "怎么做", "请问", "帮我", "如何", "怎么", "怎样", "搜索", "关于", "找找",
];

/// **词内符号白名单**：这些符号出现在技术词里是内容的一部分，不能当标点剥掉。
///
/// `C++` 剥成 `c` 会让查询命中全库（`c` 是一字宽匹配），`c#`、`node.js`、
/// `min-width`、`a/b` 同理。白名单制而不是黑名单制：标点无穷而内容符号有限，
/// 白名单漏了谁最多是少剥一个标点，黑名单漏了谁就是切坏一个正常词。
const CONTENT_SYMBOLS: &[char] = &[
    '+', '#', '.', '_', '-', '/', '\\', '~', '&', '=', '@', '$', '%', '*', '^', '|', '`',
];

/// 把原始查询串解析成检索词：trim、按空白分词、逐词去首尾标点、小写、
/// 剥检索前缀、剔停用词、去重（保序）。
///
/// **保底规则**：只要有非空白输入，就至少留下一个词 —— 全部被剔空时回退到
/// 「归一后的原始词」，归一后仍为空的（纯符号词，如 `😀`）保留原文。空词集在上游
/// 会被当成浏览列表，静默丢掉用户的筛选意图比搜不准更糟：纯符号查询宁可搜出 0 条，
/// 也不能变成「把整库倒出来」（旧路径本可 `LIKE '%😀%'` 命中）。
pub fn parse_query(raw: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    for raw_token in raw.split_whitespace() {
        let token = strip_boundary_punct(raw_token).to_lowercase();
        if token.is_empty() {
            continue;
        }
        let stripped = strip_query_prefixes(&token);
        if stripped.is_empty() || STOP_WORDS.contains(&stripped.as_str()) {
            continue;
        }
        if !tokens.iter().any(|t| t == &stripped) {
            tokens.push(stripped);
        }
    }
    if tokens.is_empty() {
        for raw_token in raw.split_whitespace() {
            let token = strip_boundary_punct(raw_token).to_lowercase();
            let token = if token.is_empty() { raw_token.to_string() } else { token };
            if !tokens.iter().any(|t| t == &token) {
                tokens.push(token);
            }
        }
    }
    tokens
}

/// 放宽召回的命中词数下限 = ⌈n/2⌉（n=2→1，n=4→2）。
///
/// 下限至少为 1：放宽的语义是"不要求全中"，不是"什么都不要求"。
pub fn relax_threshold(n: usize) -> usize {
    std::cmp::max(1, n.div_ceil(2))
}

/// `terms` 里出现在 `search_text` 中的词，按 `terms` 原顺序返回。
///
/// 逐词 `contains` 与 `search_text` 的存储口径（整篇小写）一致，判断可靠；
/// 这里刻意不复用 FTS —— 证据必须与真正返回的那条内容对得上，
/// 而 FTS 的 trigram 是"索引近似"，不是"原文子串"。
pub fn matched_terms(search_text: &str, terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .filter(|term| search_text.contains(term.as_str()))
        .cloned()
        .collect()
}

/// 去掉词首尾的标点：`!is_alphanumeric() && !CONTENT_SYMBOLS.contains(..)`。
///
/// 只动两端 —— 词内的符号保留（`min-width` / `c++` / `v1.0.7` / `a/b`）。
fn strip_boundary_punct(token: &str) -> &str {
    token.trim_matches(|ch: char| !ch.is_alphanumeric() && !CONTENT_SYMBOLS.contains(&ch))
}

/// 反复剥掉词首的检索前缀，直到没有可剥的（`请问怎么做redis` → `redis`）。
///
/// 循环而不是只剥一次：前缀会叠着出现（「请问」+「怎么做」）。
/// 只剥前缀不剥后缀 —— 「目的」「为了」也以「的/了」结尾，剥后缀会切坏正常词。
fn strip_query_prefixes(token: &str) -> String {
    let mut rest = token;
    'outer: loop {
        for prefix in QUERY_PREFIXES {
            if let Some(stripped) = rest.strip_prefix(prefix) {
                rest = stripped;
                continue 'outer;
            }
        }
        return rest.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_strips_query_prefix_and_punctuation() {
        assert_eq!(
            parse_query("请问怎么做 Redis 超时？"),
            vec!["redis", "超时"],
            "「请问怎么做」是检索前缀、问号是首尾标点，都该去掉，只留内容词"
        );
    }

    #[test]
    fn parse_query_keeps_content_words_intact() {
        assert_eq!(
            parse_query("番茄牛腩 做法"),
            vec!["番茄牛腩", "做法"],
            "「做法」不在停用词表里（宁少勿多），必须原样保留"
        );
        assert_eq!(
            parse_query("MIN-WIDTH"),
            vec!["min-width"],
            "词内连字符不是首尾标点，不能剥；大小写统一为小写"
        );
    }

    #[test]
    fn parse_query_falls_back_to_raw_tokens_when_all_stop_words() {
        assert_eq!(
            parse_query("找 一下"),
            vec!["找", "一下"],
            "全部被停用词吃掉时必须回退到原始非空词，否则查询会退化成浏览列表"
        );
    }

    #[test]
    fn parse_query_keeps_content_symbols() {
        assert_eq!(
            parse_query("C++"),
            vec!["c++"],
            "「+」是词的一部分：剥成「c」会让查询变成命中全库的单字"
        );
        assert_eq!(parse_query("C#"), vec!["c#"], "「#」同理（C# 是语言名）");
        assert_eq!(parse_query("node.js"), vec!["node.js"], "词内的点号是内容，不是句末标点");
        assert_eq!(
            parse_query("？（redis）"),
            vec!["redis"],
            "句子级标点仍要剥（问号、括号）"
        );
        assert_eq!(
            parse_query("★热点"),
            vec!["热点"],
            "与内容无关的装饰符号照旧剥掉：★ 不在词内符号白名单里"
        );
    }

    #[test]
    fn parse_query_keeps_symbol_only_tokens() {
        assert_eq!(
            parse_query("😀"),
            vec!["😀"],
            "纯符号查询必须保留原词照常检索；解析成空词集会静默变成浏览整库"
        );
        assert_eq!(parse_query("→"), vec!["→"], "箭头同理");
        assert_eq!(
            parse_query("？？？"),
            vec!["？？？"],
            "标点自成一词且剥空时保留原文：宁可搜不到，也不能静默丢掉筛选意图"
        );
    }

    #[test]
    fn parse_query_is_idempotent() {
        // list 会对 build_search_query 的结果二次 parse，幂等是硬性不变量。
        for raw in [
            "请问怎么做 Redis 超时？",
            "找 一下",
            "C++ 模板报错",
            "★热点",
            "😀",
            "？？？",
            "番茄牛腩 做法",
            "  ",
        ] {
            let once = parse_query(raw).join(" ");
            let twice = parse_query(&once).join(" ");
            assert_eq!(once, twice, "二次解析必须与一次解析结果相同：{raw:?}");
        }
    }

    #[test]
    fn parse_query_dedups_in_order_and_handles_blank_input() {
        assert_eq!(parse_query("Redis redis 超时"), vec!["redis", "超时"],
            "重复词去重，且保持用户输入的先后顺序");
        assert!(
            parse_query("   ").is_empty(),
            "纯空白解析出空词集，调用方按「浏览列表」路径处理"
        );
    }

    #[test]
    fn relax_threshold_table() {
        for (n, expect) in [(0usize, 1usize), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3)] {
            assert_eq!(relax_threshold(n), expect, "词数 {n} 的放宽阈值应为 ⌈n/2⌉ 且至少 1");
        }
    }

    #[test]
    fn matched_terms_returns_query_order() {
        let terms = vec!["番茄牛腩".to_string(), "做法".to_string()];
        assert_eq!(
            matched_terms("番茄牛腩：牛腩切块冷水下锅焯 3 分钟", &terms),
            vec!["番茄牛腩"],
            "只返回真的出现在 search_text 里的词，顺序跟随查询词序"
        );
    }
}
