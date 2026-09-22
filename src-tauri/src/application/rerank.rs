//! 本地信号重排：把召回的候选按「本地信号」重新排序（纯函数）。
//!
//! 为什么不引入 embedding：剪贴板检索的候选集很小（窗口 ≤500 条），而真正
//! 需要的判别信息（时间、命中密度、来源、分组）全在本地字段里 —— 用它们做
//! 一次确定性打分就够，零网络、零新依赖，每条排序结果都能讲清为什么。
//!
//! 定位是**纯函数**：`now` 由调用方注入，模块内不读系统时钟（同
//! [`crate::application::query_parse`] 的定位），单测因此不需要等待时间流逝。

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::application::query_parse;
use crate::domain::model::{ClipboardItem, ContentType};
use crate::domain::normalize::searchable_text;

// ---------------------------------------------------------------------------
//  权重表
//
//  五项信号各自的「量级理由」写在常量上（计划里还列过第六项「收藏」，本仓库
//  已无对应数据源，省略理由见下方 W_GROUP 之后的说明）。看表前先记住三件事，
//  否则很容易把某一项的分量想错：
//
//  1) **占比在严格路径上基本是常数，不参与区分**。严格查询的 SQL 是词间 AND，
//     正常情形下能返回的条目必然全部命中，`matched / n` 对它们都是同一个 1.0 ——
//     这一档真正的分序由「时间 + 密度 + 来源 + 分组」承担。占比只在**放宽路径**
//     （⌈n/2⌉ 阈值之上的部分命中条目）才拉开差距，那也正是「离查询意图有多近」
//     被真正比较的场景，所以它权重最高。
//     （例外：trigram 索引是近似匹配，理论上可能返回一条"索引命中但原文子串不含"
//     的条目 —— 部分命中时占比 <1；一个词都没装配到（`matched` 为空）时，
//     `match_ratio` 的防御分支按「全中」算 1.0。两种情况都是该条目自己的证据，
//     按实算即可，不需要在打分里做特殊处理。）
//  2) **辅助信号合计上限 2.5**（密度 1.0 + 来源 1.0 + 分组 0.5）：单项都 ≤1.0，
//     但合计可以超过满额时间项 2.0。因此「密度高的老条目压过一两天内的新条目」
//     是预期行为而不是 bug —— 密度（命中落在首行、反复出现）本身就是「贴题」的
//     一部分，把它让位给几天的先后反而偏离查询意图。这里**不承诺**「辅助信号
//     不推翻时间序」：叠加起来是能推翻的。
//  3) **没有 `source_app` 的条目在来源项上得 0，这是惩罚而不是中性**：它拿不到
//     聚类加成，于是稳定地比同条件下的有来源条目低一档（最多 1.0 分）。取舍是
//     这样定的：来源项的作用是识别「同一件事的上下文」，而缺失的来源没有证据
//     可以给它加分。
// ---------------------------------------------------------------------------

/// 命中占比：`matched / n` × 3.0。
///
/// 它是**放宽路径**上的主信号：多中一个词值 `3.0 / n` 分（2 词查询 1.5 分，
/// 4 词查询 0.75 分）。2 词查询里这个差距（1.5 分）大于 7 天年龄差的衰减差
/// （`2·(1-e^-0.5) ≈ 0.79` 分），所以「更贴题但更旧」能压过「更近但更不全」；
/// 但它仍小于满额时间衰减 2.0 分 —— 年龄差约 19 天（`14·ln4`）以后，半中的
/// 新条目就能反超全中的老条目。这是刻意留的口子：剪贴板是时间流，太老的内容
/// 不该只因为词全就永远置顶。
/// 严格路径上它是常数 1.0，不参与区分（见上面第 1 条）。
const W_MATCH_RATIO: f64 = 3.0;

/// 时间衰减：`exp(-age_days / 14.0)` × 2.0。
///
/// 量级理由：14 天半衰 = 剪贴板内容的典型任务周期（一件事两周内基本办完），
/// 再老的内容衰减到 0.1 以下基本退出竞争。满额 2.0 分是**辅助信号缺席**时的主要
/// 区分项；它并非不可超越的上位项（辅助信号合计上限 2.5，见上面第 2 条）。
const W_RECENCY: f64 = 2.0;
const RECENCY_HALF_LIFE_DAYS: f64 = 14.0;

/// 命中密度 ×1.0：命中词出现在首行 +0.5，出现次数子信号最多再 +0.5。
///
/// 量级理由：密度是「贴题」的一部分，满额 1.0 分相当于约 9.7 天年龄差
/// （`14·ln2`），所以压过一周内的新鲜度是预期行为。它小于放宽路径上的占比最大差
/// （2 词查询 1.5 分）—— 密度单独不足以翻过「全中 vs 半中」，但叠加来源与分组
/// 之后可以（见上面第 2 条）。
///
/// **Html 条目不给首行加成**：`searchable_text` 只剥标签、不产生换行，去标签后的
/// 「首行」往往就是整篇，照给等于系统性偏袒 Html（见 [`density_of`]）。
const W_DENSITY: f64 = 1.0;
const DENSITY_FIRST_LINE_BONUS: f64 = 0.5;
const DENSITY_OCCURRENCE_CAP: f64 = 0.5;

/// 出现次数子信号的饱和点：命中词在正文里出现 8 次即拿满 0.5。
///
/// 量级理由：剪贴板条目多数只出现一两次，8 次以上再堆重复也说明不了更相关；
/// 对数尺度保证「1 次 vs 3 次」仍有可见差别，同时防止长文本靠刷词堆分。
const DENSITY_OCCURRENCE_SATURATION: f64 = 8.0;

/// 来源聚类 ×1.0：同一 `source_app` 在候选集里的条数占比。
///
/// 量级理由：一次任务的上下文常来自同一个应用（两条 Terminal + 一条 Chrome
/// 里，Terminal 更像同一件事）。占比最大 1.0（与密度同级），与时间、密度一起
/// 参与严格路径的分序。没有来源的条目在这项得 0 —— 惩罚而非中性（见上面第 3 条）。
const W_SOURCE_CLUSTER: f64 = 1.0;

/// 分组 ×0.5：`group_id` 非空 = 用户手动归组过，价值高于普通条目。
///
/// 量级理由：分组是人工留下的强信号，但单项只有 0.5，是辅助信号里最轻的一项：
/// 单独不足以翻过时间或密度，叠加时才决定接近分的条目。
const W_GROUP: f64 = 0.5;

// 计划里还列了一项「收藏 ×0.3」，本仓库已无对应数据源：migration 4 用
// `group_id` 取代了布尔收藏（旧 `is_favorite` 列只是历史残留，新条目恒为 0，
// `ClipboardItem` 也不再有该字段）。要恢复它得动 domain 模型与 repository SQL，
// 超出本任务范围 —— 与其写一个恒为 0 的信号，不如不写。
// 用户今天要「收藏」一条内容，做法就是把它放进「收藏」分组，已被分组信号覆盖。

/// 单条候选装配好的信号：命中词与判定文本都只在这里算一次，供 [`score`] 复用。
pub struct ItemSignals<'a> {
    pub item: &'a ClipboardItem,
    /// 命中的查询词（顺序同查询词序）。**空 = 未装配到证据**，防御式按全中处理。
    pub matched: Vec<String>,
    search_text: String,
}

impl<'a> ItemSignals<'a> {
    /// 装配打分所需的信号：命中词走 [`query_parse::matched_terms`]，判定文本走
    /// [`searchable_text`]（口径依据见 `history_service::match_evidence_of` 的
    /// 不变量说明 —— 那里是同一份规则的另一处使用点，不在这里复述）。
    ///
    /// 判定文本额外被留在这份结构里：密度信号要用它数首行与出现次数，
    /// 重算一遍等于对同一条内容做两次全文小写化（单条可达 5MB）。
    pub fn assemble(item: &'a ClipboardItem, query_terms: &[String]) -> Self {
        let search_text = searchable_text(item.content_type, &item.content_text);
        let matched = query_parse::matched_terms(&search_text, query_terms);
        Self {
            item,
            matched,
            search_text,
        }
    }
}

/// 单条候选的本地信号得分。
///
/// `source_freq` 是该条目 `source_app` 在候选集里的占比（由 [`rerank`] 统一算，
/// 单条打分算不出来）；没有来源应用的条目传 0.0 —— 在这项上不得分，是稳定劣势
/// 而不是中性（见文件头第 3 条）。
pub fn score(
    signals: &ItemSignals<'_>,
    query_terms: &[String],
    now: DateTime<Utc>,
    source_freq: f64,
) -> f64 {
    let item = signals.item;

    let ratio = match_ratio(&signals.matched, query_terms.len());
    let recency = recency_of(now, item.last_copied_at);
    let density = density_of(&signals.search_text, &signals.matched, item.content_type);
    let grouped = if item.group_id.is_some() { 1.0 } else { 0.0 };

    ratio * W_MATCH_RATIO
        + recency * W_RECENCY
        + density * W_DENSITY
        + source_freq.clamp(0.0, 1.0) * W_SOURCE_CLUSTER
        + grouped * W_GROUP
}

/// 候选集重排：**排序键 =（score desc, last_copied_at desc）**。
///
/// 第二键不是装饰：分数是浮点信号算出来的，同分很常见，没有它排序就要依赖
/// 输入次序 —— 两次调用（比如说翻页）可能拿到不同的窗口顺序，用户会看到条目
/// 在页间跳动（重复或丢失）。带上 `last_copied_at` 后，同一 query 的重复调用
/// 给出同一顺序（输入序本身稳定时逐条相同）。
///
/// 本函数返回裸的条目序，供不需要命中证据的调用方使用；`list` 走的是
/// [`rerank_with_matched`]（同一套打分，顺带把命中词带回去当证据）。
pub fn rerank(
    items: Vec<ClipboardItem>,
    query_terms: &[String],
    now: DateTime<Utc>,
) -> Vec<ClipboardItem> {
    rerank_with_matched(items, query_terms, now)
        .into_iter()
        .map(|(item, _)| item)
        .collect()
}

/// 与 [`rerank`] 同一套打分，但把每条命中的词一并带回来。
///
/// 为什么要有这个版本：证据装配（`matched_terms`）与打分吃的是同一份判定，
/// 若调用方自己再算一遍，等于对窗口内每条内容做两次全文小写化（一条可能有
/// 上百 KB）。`list` 直接用这个函数，排序与证据一次拿全。
pub fn rerank_with_matched(
    items: Vec<ClipboardItem>,
    query_terms: &[String],
    now: DateTime<Utc>,
) -> Vec<(ClipboardItem, Vec<String>)> {
    let source_freq = source_frequencies(&items);

    let mut ranked: Vec<(f64, ClipboardItem, Vec<String>)> = items
        .into_iter()
        .map(|item| {
            let freq = item
                .source_app
                .as_deref()
                .and_then(|app| source_freq.get(app).copied())
                .unwrap_or(0.0);
            // 先算分与证据（借 `item`），再把它 move 进结果 —— `ItemSignals`
            // 持有 `&item`，借用必须先结束。
            let (value, matched) = {
                let signals = ItemSignals::assemble(&item, query_terms);
                (score(&signals, query_terms, now, freq), signals.matched)
            };
            (value, item, matched)
        })
        .collect();

    // `total_cmp` 而不是 `partial_cmp().unwrap()`：分数由上面的公式算出，
    // 不含 NaN，但用全序比较能保证「同分」这件事是确定的，不会因为浮点
    // 比较的边界抛出 panic。stable sort 让第二键也相等时回落到输入序。
    ranked.sort_by(|(score_a, item_a, _), (score_b, item_b, _)| {
        score_b
            .total_cmp(score_a)
            .then_with(|| item_b.last_copied_at.cmp(&item_a.last_copied_at))
    });

    ranked
        .into_iter()
        .map(|(_, item, matched)| (item, matched))
        .collect()
}

/// 命中占比。三种情况的语义都要写清，否则很容易在这一行上出静默错误：
/// - `n = 0`（没有查询词）→ 0.0：没有查询意图就没有「贴题程度」可言。
///   `list` 在空 query 时根本不调用本模块，这里是纯函数的防御边界。
/// - `matched` 为空但 `n` 非空 → 1.0 全中：这是**证据缺失**（候选都是
///   已经过 SQL 命中的条目，不该出现真正的零命中），宁可把它当贴题，
///   也不要因为证据没装配上而把一条真实命中踩到末尾。
/// - 其余 → `matched / n`（`min(1.0)` 只是防御，词集去重后不会超过 n）。
fn match_ratio(matched: &[String], n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    if matched.is_empty() {
        return 1.0;
    }
    (matched.len() as f64 / n as f64).min(1.0)
}

/// 时间衰减因子，取值 `(0, 1]`。
///
/// 未来的时间戳（时钟回拨 / 数据异常）按 age = 0 处理：不 clamp 的话
/// `exp(正数) > 1`，一条时间错乱的条目会靠时间项压过所有正常条目。
fn recency_of(now: DateTime<Utc>, last_copied_at: DateTime<Utc>) -> f64 {
    let age_days = (now - last_copied_at).num_seconds().max(0) as f64 / 86_400.0;
    (-age_days / RECENCY_HALF_LIFE_DAYS).exp()
}

/// 命中密度：首行命中 +0.5，出现次数子信号 `log(1+occ)` 归一到 [0, 0.5]。
///
/// 判定文本用 [`ItemSignals::assemble`] 里那份 `search_text`（与写库同口径），
/// 不在这里重算 —— 一条条目最多 5MB，重算两遍等于把搜索成本翻倍。
///
/// **Html 条目不给首行加成**：[`searchable_text`] 对 HTML 只剥标签、**不产生换行**，
/// 而剪贴板里的 HTML 多是被压成一行的正文 —— 去标签后的「首行」实际是整篇，
/// 照给就等于任何位置的命中都算「命中首行」，成了对 Html 的系统性偏袒。
/// 代价是 HTML 里真有换行时也拿不到这 0.5 分：宁可少给，不给假信号。
fn density_of(search_text: &str, matched: &[String], content_type: ContentType) -> f64 {
    if matched.is_empty() {
        return 0.0;
    }
    let first_line_bonus = if content_type == ContentType::Html {
        0.0
    } else {
        let first_line = search_text.split('\n').next().unwrap_or_default();
        if matched
            .iter()
            .any(|term| first_line.contains(term.as_str()))
        {
            DENSITY_FIRST_LINE_BONUS
        } else {
            0.0
        }
    };

    // 空词必须跳过：`str::matches("")` 返回「字符边界数 + 1」，会把密度刷满。
    let occurrences: usize = matched
        .iter()
        .filter(|term| !term.is_empty())
        .map(|term| search_text.matches(term.as_str()).count())
        .sum();
    let saturation = (1.0 + DENSITY_OCCURRENCE_SATURATION).ln();
    let occurrence_bonus = ((1.0 + occurrences as f64).ln() / saturation * DENSITY_OCCURRENCE_CAP)
        .min(DENSITY_OCCURRENCE_CAP);

    first_line_bonus + occurrence_bonus
}

/// 每个 `source_app` 在候选集里的出现占比。
///
/// 分母是**候选总数**（含没有来源应用的条目），不是「有来源的条目数」：
/// 后者会把「多数字段为空」误算成强聚类。没有来源的条目占比恒为 0.0 ——
/// 等效于在来源项上被扣掉最多 1.0 分，是**惩罚而非中性**（取舍见文件头第 3 条）。
fn source_frequencies(items: &[ClipboardItem]) -> HashMap<String, f64> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for item in items {
        if let Some(app) = item.source_app.as_deref() {
            *counts.entry(app).or_insert(0) += 1;
        }
    }
    let total = items.len() as f64;
    if total == 0.0 {
        return HashMap::new();
    }
    // 键取 `String`（而不是借 `items` 里的 `&str`）：重排要把 `items` 的所有权
    // 移进结果里，频率表必须活过那次 move。
    counts
        .into_iter()
        .map(|(app, count)| (app.to_string(), count as f64 / total))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::ContentType;
    use chrono::Duration;

    /// 固定基准时刻：所有用例都从这里推导时间，不读系统时钟。
    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-22T12:00:00Z")
            .expect("合法时间串")
            .with_timezone(&Utc)
    }

    fn terms(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// 构造候选的小建造者：默认文本、未分组、无来源，只改用例关心的那一两个字段。
    struct Candidate {
        content: &'static str,
        content_type: ContentType,
        age_days: f64,
        source_app: Option<&'static str>,
        grouped: bool,
    }

    impl Candidate {
        fn new(content: &'static str) -> Self {
            Self {
                content,
                content_type: ContentType::Text,
                age_days: 0.0,
                source_app: None,
                grouped: false,
            }
        }

        fn age_days(mut self, days: f64) -> Self {
            self.age_days = days;
            self
        }

        fn source_app(mut self, app: &'static str) -> Self {
            self.source_app = Some(app);
            self
        }

        fn grouped(mut self) -> Self {
            self.grouped = true;
            self
        }

        fn html(mut self) -> Self {
            self.content_type = ContentType::Html;
            self
        }

        fn build(self) -> ClipboardItem {
            // 年龄换算成毫秒：允许亚秒级差异（用于「同分」用例）。
            let copied =
                t0() - Duration::milliseconds((self.age_days * 86_400_000.0).round() as i64);
            ClipboardItem {
                id: crate::domain::model::ClipboardItemId::new(),
                content_type: self.content_type,
                content_text: self.content.to_string(),
                fingerprint: format!("fp-{}", self.content),
                group_id: self.grouped.then(|| "g-1".to_string()),
                created_at: copied,
                updated_at: copied,
                last_copied_at: copied,
                source_app: self.source_app.map(str::to_string),
                source_url: None,
                legacy_id: None,
            }
        }
    }

    /// 同命中数时新者胜（表驱动：年龄差从 1 天到 30 天都要成立）。
    #[test]
    fn newer_wins_at_equal_hit_count() {
        let query = terms(&["redis", "超时"]);
        for age_gap_days in [1.0, 7.0, 30.0] {
            let fresh = Candidate::new("redis 超时").build();
            let stale = Candidate::new("redis 超时").age_days(age_gap_days).build();
            let ranked = rerank(vec![stale.clone(), fresh.clone()], &query, t0());
            assert_eq!(
                ranked[0].id, fresh.id,
                "年龄差 {age_gap_days} 天：同命中数时新者应排前"
            );
        }
    }

    /// 同分时由第二排序键（`last_copied_at` desc）定序，而不是靠输入顺序。
    #[test]
    fn equal_scores_fall_back_to_last_copied_at() {
        let query = terms(&["redis", "超时"]);
        // 亚秒级年龄差落在同一整秒内 → 时间衰减逐位相同 → 两条分数完全相同。
        let newer = Candidate::new("redis 超时")
            .age_days(100.0 / 86_400_000.0)
            .build();
        let older = Candidate::new("redis 超时")
            .age_days(900.0 / 86_400_000.0)
            .build();

        let newer_signals = ItemSignals::assemble(&newer, &query);
        let older_signals = ItemSignals::assemble(&older, &query);
        assert_eq!(
            score(&newer_signals, &query, t0(), 0.0),
            score(&older_signals, &query, t0(), 0.0),
            "前提不成立：本用例要求两条分数完全相同；若年龄粒度变了，需要重新构造同分的一对"
        );

        // 故意反着传：只有第二排序键生效时，输出才会是时间倒序。
        let ranked = rerank(vec![older.clone(), newer.clone()], &query, t0());
        assert_eq!(
            ranked[0].id, newer.id,
            "同分时必须按 last_copied_at 倒序，不能依赖输入顺序"
        );
    }

    /// 命中占比差足以压过时间衰减（权重关系的钉子）。
    #[test]
    fn hit_ratio_outweighs_recency() {
        let query = terms(&["redis", "连接池"]);
        // 密度对齐：两条都命中首行、命中词总出现次数都是 2 → 密度项相同，
        // 差异只剩「占比」与「时间」两项，正好钉住 3.0 : 2.0 的权重关系。
        let full = Candidate::new("redis 连接池 都在这行")
            .age_days(7.0)
            .build();
        let half = Candidate::new("redis 出现了两次：redis").build();
        let ranked = rerank(vec![half.clone(), full.clone()], &query, t0());
        assert_eq!(
            ranked[0].id, full.id,
            "占比差（3.0 × 0.5 = 1.5 分）必须压过 7 天时间衰减差（≈ 0.79 分）"
        );
    }

    /// 占比按「命中几个词」分档：4 词查询里 3 中 > 2 中 > 1 中（全在首行、同时刻）。
    #[test]
    fn match_ratio_grades_by_hit_count() {
        let query = terms(&["alpha", "beta", "gamma", "delta"]);
        let three = Candidate::new("alpha beta gamma 都在这行").build();
        let two = Candidate::new("alpha beta 都在这行").build();
        let one = Candidate::new("alpha 都在这行").build();
        // 密度项（首行 +0.5）对三条都成立，出现次数也同为 1 → 排序只由占取决。
        let ranked = rerank(vec![one.clone(), two.clone(), three.clone()], &query, t0());
        let order: Vec<_> = ranked.iter().map(|item| item.id).collect();
        assert_eq!(
            order,
            vec![three.id, two.id, one.id],
            "命中词越多占比越高：4 词里 3 中应排在 2 中、1 中之前"
        );
    }

    /// 防御式规则：`matched` 为空（证据未装配）按「全中」算占比，不得被踩到末尾。
    ///
    /// 候选在 `list` 里都是**已经过 SQL 命中**的条目，真正的零命中不该出现；
    /// 万一日后有人改装配路径忘了填 `matched`，这条规则保证排序不会因此静默
    /// 退化（宁可当它贴题，也不要因为证据缺失而冤枉一条真实命中）。
    #[test]
    fn missing_evidence_is_treated_as_a_full_hit() {
        let query = terms(&["redis", "超时", "连接池"]);
        let blind = Candidate::new("这条文本里查询词一个都不出现").build();
        let blind_score = score(&ItemSignals::assemble(&blind, &query), &query, t0(), 0.0);
        assert_eq!(
            blind_score,
            W_MATCH_RATIO + W_RECENCY,
            "证据缺失时按占比 1.0 全中算：满额占比 3.0 + 满额时间 2.0，密度为 0"
        );

        // 对照组：只命中 1/3 词、同样新鲜的条目（占比 1.0 分）必须明显更低。
        let partial = Candidate::new("redis 出现在这里").build();
        let partial_score = score(&ItemSignals::assemble(&partial, &query), &query, t0(), 0.0);
        assert!(
            blind_score > partial_score,
            "证据缺失（{blind_score}）不得被 1/3 命中的条目（{partial_score}）反超"
        );
    }

    /// 首行命中比只出现在正文里更相关（密度信号）。
    #[test]
    fn first_line_hit_outranks_body_only_hit() {
        let query = terms(&["redis", "超时"]);
        let head = Candidate::new("redis 超时\n后面还有别的").build();
        let body = Candidate::new("先写点别的\nredis\n超时").build();
        let ranked = rerank(vec![body.clone(), head.clone()], &query, t0());
        assert_eq!(
            ranked[0].id, head.id,
            "同占比同时间时，命中落在首行的应排前"
        );
    }

    /// 纯文本 vs HTML 的对照：HTML 拿不到「首行命中」的 0.5，纯文本的这 0.5 还活着。
    ///
    /// 两个方向一起钉：
    /// - HTML 的 `content_text` 常是一整行（`<p>…</p>`），去标签后没有换行 ——
    ///   「首行」就是整篇，照给加成等于任何位置的命中都算命中首行。用一个命中落在
    ///   正文中部、但整篇无换行的 HTML 来验：它不得比内容等价的纯文本条目多拿分。
    /// - 纯文本里真有换行时，首行命中该拿的 0.5 必须还在（别把信号一刀砍掉）。
    #[test]
    fn html_never_takes_the_first_line_bonus() {
        let query = terms(&["redis", "超时"]);
        let body = "redis 超时 都在这里";

        // 命中在正文中部，两份内容等价（HTML 只是多了一对标签、少了那个换行）。
        let plain_body_hit = Candidate::new("先写点别的\nredis 超时 都在这里").build();
        let html_no_newline = Candidate::new("<p>先写点别的 redis 超时 都在这里</p>")
            .html()
            .build();
        // 对照：同样的正文，但命中确实落在纯文本的首行。
        let plain_first_line_hit = Candidate::new("redis 超时 都在这里\n先写点别的").build();

        let score_of =
            |item: &ClipboardItem| score(&ItemSignals::assemble(item, &query), &query, t0(), 0.0);
        let body_score = score_of(&plain_body_hit);
        let html_score = score_of(&html_no_newline);

        assert_eq!(
            html_score, body_score,
            "整篇无换行的 HTML 必须与「命中在正文里」的纯文本同分：\
             去标签后的首行 = 整篇，不能算首行命中（{body:?}）"
        );
        assert_eq!(
            score_of(&plain_first_line_hit) - body_score,
            DENSITY_FIRST_LINE_BONUS * W_DENSITY,
            "纯文本的首行加成仍在：真有换行时首行命中值 0.5"
        );
        // 出现次数在三条里相同 → 上面的分差只能来自首行那一项。
    }

    /// 来源聚类：同一 `source_app` 在候选集里占比高的条目占优。
    #[test]
    fn source_cluster_favors_the_dominant_app() {
        let query = terms(&["redis", "超时"]);
        let terminal_old = Candidate::new("redis 超时")
            .age_days(1.0)
            .source_app("Terminal")
            .build();
        let terminal_old_two = Candidate::new("redis 超时")
            .age_days(1.0)
            .source_app("Terminal")
            .build();
        let safari_fresh = Candidate::new("redis 超时").source_app("Safari").build();
        let ranked = rerank(
            vec![
                safari_fresh.clone(),
                terminal_old.clone(),
                terminal_old_two.clone(),
            ],
            &query,
            t0(),
        );
        assert_eq!(
            ranked[2].id, safari_fresh.id,
            "两条 Terminal（占 2/3）哪怕老一天，也该压过唯一的 Safari（占 1/3）"
        );
    }

    /// 分组信号：`group_id` 非空（用户手动归组过）拿到一个小的加成。
    #[test]
    fn grouped_item_gains_a_small_edge() {
        let query = terms(&["redis", "超时"]);
        let grouped = Candidate::new("redis 超时").grouped().build();
        let plain = Candidate::new("redis 超时").build();
        let ranked = rerank(vec![plain.clone(), grouped.clone()], &query, t0());
        assert_eq!(
            ranked[0].id, grouped.id,
            "同内容同时间时，已分组的条目应排前"
        );
    }

    /// 空 query_terms 是纯函数的防御边界：不得 panic、不得算出 NaN。
    #[test]
    fn empty_query_terms_are_safe() {
        let item = Candidate::new("随便什么内容").age_days(1.0).build();
        let signals = ItemSignals::assemble(&item, &[]);
        let value = score(&signals, &[], t0(), 0.0);
        assert!(value.is_finite(), "空查询词不得产生 NaN/Inf：{value}");

        let ranked = rerank(vec![item.clone()], &[], t0());
        assert_eq!(ranked.len(), 1, "空查询词也要原样返回条目（不 panic）");
        assert_eq!(ranked[0].id, item.id);
    }

    /// 时间只从注入的 `now` 来：模块内没有 `now()`。
    #[test]
    fn recency_reads_the_injected_now() {
        let query = terms(&["redis", "超时"]);
        let item = Candidate::new("redis 超时").build();
        let signals = ItemSignals::assemble(&item, &query);

        let at_t0 = score(&signals, &query, t0(), 0.0);
        let month_later = score(&signals, &query, t0() + Duration::days(30), 0.0);
        assert!(
            at_t0 > month_later,
            "now 越晚则时间衰减越小：{at_t0} vs {month_later}"
        );
        assert_eq!(
            score(&signals, &query, t0(), 0.0),
            at_t0,
            "同一 now 必须逐位相同（无隐藏时钟）"
        );
    }

    /// 命中词口径与写库的 `search_text` 一致：HTML 先剥标签再判命中。
    #[test]
    fn html_signals_use_the_same_text_as_the_search_index() {
        let html = Candidate::new("<p>番茄牛腩</p>").html().build();
        let signals = ItemSignals::assemble(&html, &terms(&["番茄牛腩", "p"]));
        assert_eq!(
            signals.matched,
            vec!["番茄牛腩".to_string()],
            "标签名不是内容，不计命中（与写库 search_text 的去标签口径一致）"
        );
    }

    /// 未来时间戳（时钟回拨 / 脏数据）不得换来超额的衰减加成。
    #[test]
    fn future_timestamps_are_clamped_to_the_fresh_end() {
        let query = terms(&["redis", "超时"]);
        let fresh = Candidate::new("redis 超时").build();
        let future = Candidate::new("redis 超时").age_days(-5.0).build();

        let fresh_signals = ItemSignals::assemble(&fresh, &query);
        let future_signals = ItemSignals::assemble(&future, &query);
        assert_eq!(
            score(&future_signals, &query, t0(), 0.0),
            score(&fresh_signals, &query, t0(), 0.0),
            "未来的时间戳按 age=0 处理：时间衰减因子封顶在 1.0"
        );
    }
}
