//! 会话生成：把候选条目聚成会话，并为每一条写清「为什么在这里」。
//!
//! 全部是**纯函数**：不碰数据库、不读设置、不发网络请求 —— 调用方负责取候选
//! （`SessionService::refresh_and_list`）和落库（`AgentSessionStore`）。
//!
//! 三条硬条件（关键判断 6）是这里唯一的聚类依据，来源与共享词只做措辞不做筛选：
//! ① 与前一条的复制间隔 ≤ [`SESSION_MAX_GAP_MINUTES`]；
//! ② 与前一条同源（双方都是「未知来源」也算同源）**或**至少共享 1 个内容词；
//! ③ 单会话最多 [`SESSION_MAX_ITEMS`] 条，到顶切断。
//!
//! 措辞一律用**绝对本地时间**（`14:02` / 跨天写 `09-21 22:10`），不用「N 分钟前」：
//! 相对措辞会让同一次重算在不同时刻得到不同文本，与「重算幂等」直接冲突。

use chrono::{DateTime, Local, Utc};
use sha2::{Digest, Sha256};

use crate::application::agent_service::MAX_AGENT_INPUT_ITEMS;
use crate::application::query_parse::STOP_WORDS;
use crate::domain::model::{
    AgentSessionDraft, AgentSessionItemDraft, ClipboardItem, ContentType, SessionSource,
};
use crate::domain::normalize::searchable_text;

/// 相邻两条能留在同一个会话里的最大间隔（分钟）。
///
/// 量级理由：剪贴板是连续工作流的流水，半小时是「一段连续操作」的粒度 ——
/// 再短会把「查资料 → 试命令 → 记笔记」这条链切碎，再长会把午休前后的两件事
/// 缝在一起。检索侧的 `rerank` 用 14 天半衰（宽容窗口），与会话无关。
pub const SESSION_MAX_GAP_MINUTES: i64 = 30;

/// 一个会话至少几条成员。
///
/// 一条不成组：单条会话等于把历史列表重排一遍，没有信息量。
pub const SESSION_MIN_ITEMS: usize = 2;

/// 一个会话最多几条成员。
///
/// **直接引用** `MAX_AGENT_INPUT_ITEMS`（关键判断 8）：两者是同一个数而不是两个
/// 20 —— 用户看到一条 25 条的会话、点「AI 归纳」必然撞 `ai_too_many_items`，
/// 那说明分组本身就越界了。
pub const SESSION_MAX_ITEMS: usize = MAX_AGENT_INPUT_ITEMS;

/// 内容词个数上限。
///
/// 一条内容词表只用来算「共享关键词」与标题，32 个远多于任何一条真实剪贴内容
/// 需要被解释的词数 —— 截断保序，先出现的词更可能是主题。
const MAX_CONTENT_TERMS: usize = 32;

/// 内容词的长度窗口（按字符计，不是字节）。
///
/// 下限 2：单字（含单个汉字 / 单个字母）在任何文本里都近乎必然出现，共享它不构成
/// 「这两条在讲同一件事」的证据。上限 24：超过这个长度的整串多半是没被切开的
/// 句子（中文无空格时最典型），把它当「一个词」写进标题没有可读性。
const CONTENT_TERM_MIN_CHARS: usize = 2;
const CONTENT_TERM_MAX_CHARS: usize = 24;

/// 标题里写几个共享词。
const TITLE_TERM_LIMIT: usize = 2;

/// 摘要里「来源」段的展示个数上限，超出改写「等 N 个来源」。
const SUMMARY_SOURCE_LIMIT: usize = 3;

/// 摘要里写几个共享关键词。
const SUMMARY_TERM_LIMIT: usize = 2;

/// 从一条内容里抽出「内容词」：与 [`crate::application::query_parse::parse_query`]
/// 同源的规则，但**不做查询前缀剥离**。
///
/// 为什么同源：内容词与检索词必须是同一套口径，否则「界面上说这两条共享
/// `redis`」与「搜索 `redis` 命中它」会各说各话。为什么**不**剥前缀：
/// `请问redis` 里的「请问」是正文的一部分（那是别人粘贴的一段文字），
/// 只有用户亲手敲的查询才有「请问」这种提问前缀。
///
/// 切分依据是**空白与标点**（关键判断 7）：标点本身不是内容，`redis，超时` 是
/// 两个词；而词内符号白名单（`.` `-` `+` …）里的符号属于技术词的一部分，
/// 参与切分就会把 `node.js` 切成两半、把 `c++` 切废。中文长句没有空格、
/// 也切不出这类技术词，于是中文条目主要靠时间与来源成组 —— 这是诚实取舍：
/// 中文分词要么引词典依赖、要么调模型，与「零依赖 + 本地优先」冲突。
///
/// 规则：`searchable_text`（HTML 去标签 + 小写）→ 按空白 / 标点切分 →
/// 只留长度 `2..=24` 且含字母数字的 token → 剔 `STOP_WORDS` → 去重保序，
/// 上限 32。
pub fn content_terms(text: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for raw_token in split_on_punctuation(text) {
        let token = raw_token.to_lowercase();
        if token.is_empty() || token.chars().count() < CONTENT_TERM_MIN_CHARS {
            continue;
        }
        // 长度上界按**字符**算（中文一个字算一个），并且必须含字母或数字：
        // 纯符号 token 不是内容词。
        if token.chars().count() > CONTENT_TERM_MAX_CHARS {
            continue;
        }
        if !token.chars().any(char::is_alphanumeric) {
            continue;
        }
        if STOP_WORDS.contains(&token.as_str()) {
            continue;
        }
        if !terms.iter().any(|existing| existing == &token) {
            terms.push(token);
        }
        if terms.len() >= MAX_CONTENT_TERMS {
            break;
        }
    }
    terms
}

/// 按空白与标点把文本切成 token：空白与「不在词内符号白名单里的符号」都是分隔符。
///
/// 与 `query_parse::strip_boundary_punct` 同一套「什么算标点」的判定
/// （`!is_alphanumeric() && !CONTENT_SYMBOLS.contains(..)`），区别只在用途：
/// 那边剥词的首尾，这边在词内部也切开。
fn split_on_punctuation(text: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = Vec::new();
    let mut start: Option<usize> = None;
    for (index, ch) in text.char_indices() {
        let is_content = ch.is_alphanumeric() || is_content_symbol(ch);
        match (is_content, start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                tokens.push(&text[begin..index]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        tokens.push(&text[begin..]);
    }
    tokens
}

/// 词内符号白名单：这些符号出现在技术词里是内容的一部分，不能当首尾标点剥掉。
///
/// 与 `query_parse::CONTENT_SYMBOLS` 同一份清单，但**故意不共享常量**：
/// 两处的可见性诉求不同（那边是私有实现细节、这里是内容词规则），
/// 而这份清单本身比它出现的函数更稳定 —— 真要改，两边都得看一眼。
fn is_content_symbol(ch: char) -> bool {
    matches!(
        ch,
        '+' | '#'
            | '.'
            | '_'
            | '-'
            | '/'
            | '\\'
            | '~'
            | '&'
            | '='
            | '@'
            | '$'
            | '%'
            | '*'
            | '^'
            | '|'
            | '`'
    )
}

/// 取一条成员的内容词：只有文本类内容贡献内容词。
///
/// 图片 / 文件条目的 `content_text` 是**路径**（`/Users/liang/Pictures/a.png`），
/// 路径片段不是内容：否则同一台机器上的两张截图永远「共享 users / pictures」，
/// 理由会写出没有意义的句子。HTML 走 `searchable_text` 去标签。
fn item_terms(item: &ClipboardItem) -> Vec<String> {
    match item.content_type {
        ContentType::Text | ContentType::Html => {
            content_terms(&searchable_text(item.content_type, &item.content_text))
        }
        ContentType::Image | ContentType::Files => Vec::new(),
    }
}

/// 会话内按「出现条数 desc、首次出现位置 asc、词 asc」排序的共享词。
///
/// 只保留出现在 **≥2 条**成员里的词：只出现一次的词不能证明「这几条在讲同一件事」。
/// 这个排序同时服务标题（取前 2）与摘要（取前 2），保证两处写出来的词一致 ——
/// 用户能自己核对「标题里的词」就是「理由里的词」。
fn shared_terms(group: &[(usize, Vec<String>)]) -> Vec<String> {
    let mut counts: Vec<(String, usize, usize)> = Vec::new();
    for (order, terms) in group {
        for term in terms {
            match counts.iter_mut().find(|(existing, ..)| existing == term) {
                Some((_, count, _)) => *count += 1,
                None => counts.push((term.clone(), 1, *order)),
            }
        }
    }
    counts.retain(|(_, count, _)| *count >= 2);
    counts.sort_by(
        |(left, left_count, left_order), (right, right_count, right_order)| {
            right_count
                .cmp(left_count)
                .then_with(|| left_order.cmp(right_order))
                .then_with(|| left.cmp(right))
        },
    );
    counts.into_iter().map(|(term, ..)| term).collect()
}

/// 会话内出现过的来源应用名，去重保序（按成员顺序）。
///
/// 未知来源（`source_app = None`）不进列表 —— 它没有名字可写；全部未知时
/// 摘要不写来源段，而不是写一个空列表。
fn sources_in_order(group: &[ClipboardItem]) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    for item in group {
        if let Some(app) = item.source_app.as_deref() {
            if !sources.iter().any(|existing| existing == app) {
                sources.push(app.to_string());
            }
        }
    }
    sources
}

/// 摘要里的时刻（起点）：`MM-DD HH:MM`。
///
/// 起点总带日期，会话属于哪一天由它交代；同一天的终点只写 `HH:MM`，
/// 免得一行摘要里日期写两遍。
fn format_stamp(moment: DateTime<Utc>) -> String {
    moment
        .with_timezone(&Local)
        .format("%m-%d %H:%M")
        .to_string()
}

/// 摘要里的时刻（终点）：与起点同一天只写 `HH:MM`，跨天则同样写 `MM-DD HH:MM`。
///
/// 跨天必须补齐日期：`09-21 23:58–00:05` 看起来像时间倒着走。
fn format_end_stamp(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    let local_start = start.with_timezone(&Local);
    let local_end = end.with_timezone(&Local);
    if local_start.date_naive() == local_end.date_naive() {
        local_end.format("%H:%M").to_string()
    } else {
        local_end.format("%m-%d %H:%M").to_string()
    }
}

/// 理由里的时刻：只写 `HH:MM`。
///
/// 理由紧跟标题 / 摘要出现，日期由摘要给出；同一时段内逐条重复日期会把一行证据
/// 挤满（理由要在一行里说清同源 / 共享词 / 间隔三件事）。
fn format_clock(moment: DateTime<Utc>) -> String {
    moment.with_timezone(&Local).format("%H:%M").to_string()
}

/// 标题与摘要（两条路径共用同一套措辞）。
///
/// 标题三级回退：共享词 → 唯一来源 → 条数；摘要 = 时段 + 条数 + 来源段 + 共享词段。
/// 全部由会话内的**事实**拼出，不含任何模型产出（决策 1）。
pub fn describe(items: &[ClipboardItem]) -> (String, String) {
    if items.is_empty() {
        // 上游卡着 `SESSION_MIN_ITEMS`，正常路径到不了这里；返回一个诚实的空描述
        // 而不是 panic —— 纯函数不该因为空输入把整个工作台掀翻。
        return ("0 条内容".to_string(), String::new());
    }

    let indexed: Vec<(usize, Vec<String>)> = items
        .iter()
        .enumerate()
        .map(|(order, item)| (order, item_terms(item)))
        .collect();
    let shared = shared_terms(&indexed);
    let sources = sources_in_order(items);
    let count = items.len();

    let title = if !shared.is_empty() {
        let words: Vec<&str> = shared
            .iter()
            .take(TITLE_TERM_LIMIT)
            .map(String::as_str)
            .collect();
        format!("{} · {count} 条", words.join(" · "))
    } else if sources.len() == 1 {
        format!("{} · {count} 条", sources[0])
    } else {
        format!("{count} 条内容")
    };

    let first = items.first().expect("上面已排除空输入");
    let last = items.last().expect("上面已排除空输入");
    let span = format!(
        "{}–{}",
        format_stamp(first.last_copied_at),
        format_end_stamp(first.last_copied_at, last.last_copied_at)
    );
    // 时长按整分钟向下取整：措辞要的是量级（40 分钟），不是精确到秒的答案。
    let minutes = (last.last_copied_at - first.last_copied_at)
        .num_minutes()
        .max(0);

    let mut summary = format!("{span}（{minutes} 分钟）的 {count} 条内容");
    if !sources.is_empty() {
        let listed: Vec<&str> = sources
            .iter()
            .take(SUMMARY_SOURCE_LIMIT)
            .map(String::as_str)
            .collect();
        summary.push_str(&format!("；来源：{}", listed.join("、")));
        if sources.len() > SUMMARY_SOURCE_LIMIT {
            summary.push_str(&format!(" 等 {} 个来源", sources.len()));
        }
    }
    if !shared.is_empty() {
        let words: Vec<&str> = shared
            .iter()
            .take(SUMMARY_TERM_LIMIT)
            .map(String::as_str)
            .collect();
        summary.push_str(&format!("；共享关键词：{}", words.join("、")));
    }
    summary.push('。');

    (title, summary)
}

/// 会话 id：由成员集合确定性派生（关键判断 4）。
///
/// `sess-` + `hex(sha256(排序后的 item_id 逐行拼接))[..32]`。同一成员集合 → 同一 id，
/// 未变化的会话在重算前后是同一行（界面不跳）；加了新条目则成一个新会话，
/// 旧的那个在同一次重算里被清掉。不引 uuid v5（那要开 feature），
/// 用已在依赖树里的 `sha2` + `hex`。
fn session_id(item_ids: &[String]) -> String {
    let mut hasher = Sha256::new();
    for item_id in item_ids {
        hasher.update(item_id.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    format!("sess-{}", &hex::encode(digest)[..32])
}

/// 第 1 条成员的理由：起点，没有「与前一条」的证据可写。
fn start_reason(item: &ClipboardItem) -> String {
    let app = item.source_app.as_deref().unwrap_or("未知来源");
    format!(
        "会话起点（{} · {}）",
        app,
        format_clock(item.last_copied_at)
    )
}

/// 第 k 条成员的理由：按证据三选一（同源 + 共享词 / 只有共享词 / 只有同源）。
///
/// 链式措辞（相对前一条）而不是全部相对第 1 条 —— 相邻两条的证据才是成组那一刻
/// 真实成立的证据；摘要里的「来源 / 共享关键词」是整组的全局证据，
/// 用户能自己核对传递性。
///
/// 措辞规格逐字（计划的例子）：
/// `与前一条同源（iTerm2）· 共享「redis」「超时」· 相隔 3 分钟`（两条证据都有）
/// `与前一条共享「redis」· 相隔 8 分钟`（跨应用，只有共享词）
/// `与前一条同源（Chrome）· 相隔 5 分钟`（同源，但没有共享词）
/// —— 同源在时，共享段省掉「与前一条」（前半句已经交代了参照系）。
///
/// 构造出的理由**一定 trim 后非空**：数据库的条件 CHECK 只剥 ASCII 空格
/// （契约字面如此），构造侧先保证一遍 —— 「三处钉住」的第三处。成组时
/// 证据必有其一（硬条件②），所以不存在「无证据」的分支。
fn step_reason(
    previous: &ClipboardItem,
    current: &ClipboardItem,
    shared_with_previous: &[String],
) -> String {
    let same_source = previous.source_app == current.source_app;
    let minutes = (current.last_copied_at - previous.last_copied_at)
        .num_minutes()
        .max(0);

    let mut parts: Vec<String> = Vec::new();
    if same_source {
        let app = current.source_app.as_deref().unwrap_or("未知来源");
        parts.push(format!("与前一条同源（{app}）"));
    }
    if !shared_with_previous.is_empty() {
        let words: Vec<String> = shared_with_previous
            .iter()
            .map(|term| format!("「{term}」"))
            .collect();
        let prefix = if same_source { "" } else { "与前一条" };
        parts.push(format!("{prefix}共享{}", words.join("")));
    }
    parts.push(format!("相隔 {minutes} 分钟"));
    // 分隔符是「· 」而不是「 · 」：理由是一行紧凑的证据链，前置空格会把
    // 「与前一条」这一组证据切开读；形态照计划的例子逐字。
    parts.join("· ")
}

/// 把候选条目聚成会话（`source = Agent`）。
///
/// 输入顺序无关：内部先按 `(last_copied_at, id)` 升序排序，同一时刻的多条有稳定序。
/// `last_copied_at > now` 的脏数据（时钟回拨 / 手改库）被丢弃 —— 它既不该进会话，
/// 也不该把一条正常内容拽成孤独的单条。
pub fn build(items: &[ClipboardItem], now: DateTime<Utc>) -> Vec<AgentSessionDraft> {
    let mut sorted: Vec<&ClipboardItem> = items
        .iter()
        .filter(|item| item.last_copied_at <= now)
        .collect();
    sorted.sort_by(|left, right| {
        left.last_copied_at
            .cmp(&right.last_copied_at)
            .then_with(|| left.id.to_string().cmp(&right.id.to_string()))
    });

    let mut drafts: Vec<AgentSessionDraft> = Vec::new();
    let mut group: Vec<&ClipboardItem> = Vec::new();

    for item in sorted {
        let joins = match group.last() {
            None => false,
            Some(previous) => {
                let within_window = (item.last_copied_at - previous.last_copied_at).num_minutes()
                    <= SESSION_MAX_GAP_MINUTES;
                // 条件②是二选一：同源（双方都是「未知来源」也算同源）**或**共享内容词。
                // 不要求「整组同源」：跨应用是工作流最常见的形态，把来源相同做成硬条件
                // 会把「查资料 → 试命令 → 记笔记」这条链切碎。
                let evidence = previous.source_app == item.source_app
                    || !shared_between(previous, item).is_empty();
                within_window && evidence && group.len() < SESSION_MAX_ITEMS
            }
        };

        if !joins && !group.is_empty() {
            if let Some(draft) = finish_agent_group(&mut group) {
                drafts.push(draft);
            }
        }
        group.push(item);
    }
    if let Some(draft) = finish_agent_group(&mut group) {
        drafts.push(draft);
    }
    drafts
}

/// 两条之间的共享内容词（按前一条的内容词顺序）。
fn shared_between(previous: &ClipboardItem, current: &ClipboardItem) -> Vec<String> {
    let previous_terms = item_terms(previous);
    let current_terms = item_terms(current);
    previous_terms
        .into_iter()
        .filter(|term| current_terms.iter().any(|other| other == term))
        .collect()
}

/// 收尾一组：少于 [`SESSION_MIN_ITEMS`] 条不成会话（`None`），
/// 否则生成草稿（成员 1-based `position`、逐条理由）。
fn finish_agent_group(group: &mut Vec<&ClipboardItem>) -> Option<AgentSessionDraft> {
    if group.len() < SESSION_MIN_ITEMS {
        group.clear();
        return None;
    }

    let members: Vec<ClipboardItem> = group.iter().map(|item| (*item).clone()).collect();
    let item_ids: Vec<String> = members.iter().map(|item| item.id.to_string()).collect();
    let (title, summary) = describe(&members);

    let mut items: Vec<AgentSessionItemDraft> = Vec::with_capacity(members.len());
    for (index, member) in members.iter().enumerate() {
        let reason = if index == 0 {
            start_reason(member)
        } else {
            let previous = &members[index - 1];
            step_reason(previous, member, &shared_between(previous, member))
        };
        // 理由交给 draft 之前再过一道：数据库的 CHECK 只剥 ASCII 空格，
        // 空 / 全空白理由在这里就被挡下，而不是留给一次写入失败。
        let reason = reason.trim().to_string();
        debug_assert!(!reason.is_empty(), "agent 源的每条成员都必须说得清理由");
        items.push(AgentSessionItemDraft {
            item_id: member.id.to_string(),
            position: index as i64 + 1,
            reason: Some(reason),
        });
    }

    let draft = AgentSessionDraft {
        id: session_id(&item_ids),
        source: SessionSource::Agent,
        title,
        summary,
        created_at: members.first()?.last_copied_at,
        updated_at: members.last()?.last_copied_at,
        items,
    };
    group.clear();
    Some(draft)
}

/// 用户显式保存的会话（`source = User`）。
///
/// 单组不切分：用户亲手选的人选不需要时间 / 来源门槛（他为什么把它们放在一起，
/// 他自己知道），`reason = None` —— 只有自动成组才需要解释。
///
/// 唯一的时间处理：未来的脏时间戳**不丢成员**（静默丢掉他选中的人选更糟），
/// 只把会话行的时间夹回 `now` —— 否则列表按 `updated_at DESC` 会把这条会话
/// 永久钉在顶端。成员顺序按 `last_copied_at` 升序（与 agent 路径同口径）。
pub fn build_user_session(items: &[ClipboardItem], now: DateTime<Utc>) -> AgentSessionDraft {
    let mut sorted: Vec<&ClipboardItem> = items.iter().collect();
    sorted.sort_by(|left, right| {
        left.last_copied_at
            .cmp(&right.last_copied_at)
            .then_with(|| left.id.to_string().cmp(&right.id.to_string()))
    });

    let members: Vec<ClipboardItem> = sorted.iter().map(|item| (*item).clone()).collect();
    let item_ids: Vec<String> = members.iter().map(|item| item.id.to_string()).collect();
    let (title, summary) = describe(&members);

    let items: Vec<AgentSessionItemDraft> = members
        .iter()
        .enumerate()
        .map(|(index, member)| AgentSessionItemDraft {
            item_id: member.id.to_string(),
            position: index as i64 + 1,
            reason: None,
        })
        .collect();

    let latest = members
        .last()
        .map(|item| item.last_copied_at)
        .unwrap_or(now)
        .min(now);
    let earliest = members
        .first()
        .map(|item| item.last_copied_at)
        .unwrap_or(now)
        .min(now);

    AgentSessionDraft {
        id: session_id(&item_ids),
        source: SessionSource::User,
        title,
        summary,
        created_at: earliest,
        updated_at: latest,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use std::str::FromStr;

    use crate::domain::model::ClipboardItemId;

    /// 固定的条目 id：唯一性靠尾号，顺序断言靠十六进制序。
    fn item_id(tail: u8) -> ClipboardItemId {
        ClipboardItemId::from_str(&format!("00000000-0000-4000-8000-0000000000{tail:02}"))
            .expect("合法 uuid")
    }

    /// 本地墙钟 → UTC。措辞规格里是**本地时间**（`14:02`），用本地墙钟造输入，
    /// 断言的字面量才能在任意时区都成立。
    fn local(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Local
            .with_ymd_and_hms(2026, 9, day, hour, minute, 0)
            .single()
            .expect("本地时间唯一")
            .with_timezone(&Utc)
    }

    /// 用例里的「现在」：固定 2026-09-22 15:00 本地 —— 晚于所有正常用例的时间戳，
    /// 早于「未来时间戳」那条用例里刻意造的脏数据。
    fn now() -> DateTime<Utc> {
        local(22, 15, 0)
    }

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        local(22, hour, minute)
    }

    fn item(tail: u8, app: Option<&str>, text: &str, copied_at: DateTime<Utc>) -> ClipboardItem {
        ClipboardItem {
            id: item_id(tail),
            content_type: ContentType::Text,
            content_text: text.to_string(),
            fingerprint: format!("fp-{tail}"),
            group_id: None,
            created_at: copied_at,
            updated_at: copied_at,
            last_copied_at: copied_at,
            source_app: app.map(str::to_string),
            source_url: None,
            legacy_id: None,
        }
    }

    fn html_item(tail: u8, app: &str, html: &str, copied_at: DateTime<Utc>) -> ClipboardItem {
        ClipboardItem {
            content_type: ContentType::Html,
            ..item(tail, Some(app), html, copied_at)
        }
    }

    fn image_item(tail: u8, app: &str, path: &str, copied_at: DateTime<Utc>) -> ClipboardItem {
        ClipboardItem {
            content_type: ContentType::Image,
            ..item(tail, Some(app), path, copied_at)
        }
    }

    /// 草稿的可观察形状（含成员理由）的字符串快照：断言「输入顺序无关」与
    /// 「同一输入两次调用逐字相等」时，逐字比对整串比逐个字段断言更不容易写漏。
    fn snapshot(drafts: &[AgentSessionDraft]) -> Vec<String> {
        drafts
            .iter()
            .map(|draft| {
                let members: Vec<String> = draft
                    .items
                    .iter()
                    .map(|member| {
                        format!("{}@{}={:?}", member.item_id, member.position, member.reason)
                    })
                    .collect();
                format!(
                    "{}|{}|{}|{}|{}|{}",
                    draft.id,
                    draft.title,
                    draft.summary,
                    draft.created_at.to_rfc3339(),
                    draft.updated_at.to_rfc3339(),
                    members.join(",")
                )
            })
            .collect()
    }

    fn member_ids(draft: &AgentSessionDraft) -> Vec<String> {
        draft
            .items
            .iter()
            .map(|member| member.item_id.clone())
            .collect()
    }

    fn reason_of(draft: &AgentSessionDraft, position: usize) -> &str {
        draft.items[position - 1]
            .reason
            .as_deref()
            .expect("agent 源每条成员都要有理由")
    }

    // -----------------------------------------------------------------------
    //  内容词
    // -----------------------------------------------------------------------

    /// 规则与 `query_parse::parse_query` 同源：去首尾标点、小写、剔同一张停用词表；
    /// 但**不做查询前缀剥离** —— 这是内容，不是用户敲的查询。
    #[test]
    fn content_terms_follow_the_query_word_rules_without_prefix_stripping() {
        assert_eq!(
            content_terms("请问 Redis，超时？ C++ a 的 redis"),
            vec!["redis", "超时", "c++"],
            "「请问」「的」是停用词、单字 a 太短、首尾标点要剥，去重保序"
        );
        assert_eq!(
            content_terms("请问redis"),
            vec!["请问redis"],
            "不做检索前缀剥离：词内连着写的前缀是正文的一部分（查询词才剥）"
        );
        assert_eq!(
            content_terms("Node.js MIN-WIDTH"),
            vec!["node.js", "min-width"],
            "词内符号（. / -）是内容的一部分，不能当标点剥掉"
        );
    }

    #[test]
    fn content_terms_drop_tokens_outside_the_length_window_and_cap_the_list() {
        assert!(
            content_terms(&"a".repeat(25)).is_empty(),
            "超过 24 字符的长串多半是没有空格的整句，不算内容词"
        );
        assert_eq!(content_terms(&"b".repeat(24)).len(), 1, "24 字符仍在窗口内");

        let many = (0..40)
            .map(|index| format!("term{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let terms = content_terms(&many);
        assert_eq!(terms.len(), 32, "内容词上限 32 个");
        assert_eq!(terms[0], "term0", "留的是先出现的那些（保序截断）");
    }

    // -----------------------------------------------------------------------
    //  聚类（关键判断 6 的三条硬条件）
    // -----------------------------------------------------------------------

    /// 时间窗是硬条件①：31 分钟切断成两段，正好 30 分钟仍算同一段。
    #[test]
    fn gaps_beyond_the_window_split_the_session() {
        let split = [
            item(1, Some("iTerm2"), "redis 超时", at(14, 0)),
            item(2, Some("iTerm2"), "redis 超时", at(14, 10)),
            item(3, Some("iTerm2"), "redis 超时", at(14, 41)),
            item(4, Some("iTerm2"), "redis 超时", at(14, 45)),
        ];
        let drafts = build(&split, now());
        assert_eq!(drafts.len(), 2, "31 分钟的间隔把流水切成两段");
        assert_eq!(
            member_ids(&drafts[0]),
            vec![item_id(1).to_string(), item_id(2).to_string()]
        );
        assert_eq!(
            member_ids(&drafts[1]),
            vec![item_id(3).to_string(), item_id(4).to_string()]
        );
        assert_eq!(
            reason_of(&drafts[1], 1),
            "会话起点（iTerm2 · 14:41）",
            "切断后的第一条重新是起点"
        );

        let boundary = [
            item(5, Some("iTerm2"), "redis 超时", at(14, 0)),
            item(6, Some("iTerm2"), "redis 超时", at(14, 30)),
        ];
        assert_eq!(
            build(&boundary, now()).len(),
            1,
            "正好 30 分钟仍在窗内（「≤ 30 分钟」）"
        );
    }

    /// 条件②是二选一：跨应用但有共享词 → 同组；跨应用且没有共享词 → 断开。
    #[test]
    fn cross_app_items_need_a_shared_term_to_stay_together() {
        let together = [
            item(1, Some("iTerm2"), "redis 超时", at(14, 0)),
            item(2, Some("Chrome"), "redis 排查", at(14, 5)),
        ];
        let drafts = build(&together, now());
        assert_eq!(drafts.len(), 1, "跨应用但有共享词 → 同一段工作流");
        assert_eq!(
            reason_of(&drafts[0], 2),
            "与前一条共享「redis」· 相隔 5 分钟",
            "跨应用时只写共享词这条证据"
        );

        let apart = [
            item(3, Some("iTerm2"), "部署脚本", at(14, 0)),
            item(4, Some("Chrome"), "会议纪要", at(14, 5)),
        ];
        assert!(
            build(&apart, now()).is_empty(),
            "跨应用且无共享词 → 断开，两条各自都不成会话"
        );
    }

    /// 输入顺序无关：内部按 `(last_copied_at, id)` 升序；同一时刻的多条按 id 稳定排序。
    #[test]
    fn input_order_does_not_matter_and_equal_timestamps_sort_by_id() {
        let items = vec![
            item(1, Some("iTerm2"), "redis 超时", at(14, 0)),
            item(2, Some("Chrome"), "redis", at(14, 5)),
            item(3, Some("iTerm2"), "redis 排查", at(14, 10)),
        ];
        let mut reversed = items.clone();
        reversed.reverse();
        assert_eq!(
            snapshot(&build(&items, now())),
            snapshot(&build(&reversed, now())),
            "乱序输入与正序输入结果逐字相等"
        );

        let same_time = vec![
            item(5, Some("Chrome"), "alpha", at(14, 30)),
            item(4, Some("Chrome"), "beta", at(14, 30)),
        ];
        let drafts = build(&same_time, now());
        assert_eq!(
            member_ids(&drafts[0]),
            vec![item_id(4).to_string(), item_id(5).to_string()],
            "同一时刻的多条按 id 升序（输入顺序是倒着的，结果仍按 id）"
        );
    }

    /// 脏数据：`last_copied_at > now` 的条目（时钟回拨 / 手改库）不进会话，
    /// 也不该把一条正常内容拽成孤独的单条。
    ///
    /// fixture 刻意让所有时间戳**彼此都在 30 分钟窗内**，唯一差别是第二条落在
    /// `now` 之后：删掉未来过滤，13:50 与 14:20 就会成组。旧 fixture 把未来条
    /// 放在 5 小时之外 —— 靠时间窗也能挡住它，测不出这条过滤。
    #[test]
    fn future_timestamps_are_dropped() {
        let now_14 = local(22, 14, 0);
        let pair = [
            item(1, Some("iTerm2"), "redis 超时", at(13, 50)),
            item(2, Some("iTerm2"), "redis 超时", at(14, 20)),
        ];
        assert!(
            build(&pair, now_14).is_empty(),
            "14:20 来自未来被丢弃；13:50 剩一条不成会话（不过滤就会成组）"
        );

        let mixed = [
            item(3, Some("iTerm2"), "redis 超时", at(13, 50)),
            item(4, Some("iTerm2"), "redis 超时", at(13, 55)),
            item(5, Some("iTerm2"), "redis 超时", at(14, 20)),
        ];
        let drafts = build(&mixed, now_14);
        assert_eq!(drafts.len(), 1);
        assert_eq!(
            member_ids(&drafts[0]),
            vec![item_id(3).to_string(), item_id(4).to_string()],
            "未来时间戳不进会话（不过滤就会多出第三个成员）"
        );
    }

    /// 条件③：单会话上限 = 一次批量归纳的上限（关键判断 8）；到顶切断，
    /// 新会话重新从 1 编号（新会话的第一条是起点，不需要证据）。
    #[test]
    fn sessions_are_cut_at_the_item_cap() {
        let items: Vec<ClipboardItem> = (0..22)
            .map(|index| {
                item(
                    index as u8 + 1,
                    Some("iTerm2"),
                    "redis 超时",
                    at(14, index as u32),
                )
            })
            .collect();
        let drafts = build(&items, now());
        assert_eq!(drafts.len(), 2, "22 条切成「20 条 + 2 条」");
        assert_eq!(drafts[0].items.len(), SESSION_MAX_ITEMS);
        assert_eq!(drafts[1].items.len(), 2);
        assert_eq!(drafts[1].items[0].position, 1);
        assert_eq!(reason_of(&drafts[1], 1), "会话起点（iTerm2 · 14:20）");
        assert!(
            drafts
                .iter()
                .all(|draft| draft.items.len() >= SESSION_MIN_ITEMS),
            "0 成员 / 单成员会话不可达"
        );
    }

    /// 一条不成组：单条会话等于把历史列表重排一遍，没有信息量。
    #[test]
    fn a_single_item_never_becomes_a_session() {
        assert!(build(&[], now()).is_empty(), "空输入");
        assert!(
            build(&[item(1, Some("iTerm2"), "redis", at(14, 0))], now()).is_empty(),
            "孤立的一条"
        );
        // 空输入在 describe 里不可达（上游卡着 SESSION_MIN_ITEMS），这里只钉住「不 panic」。
        assert_eq!(describe(&[]), ("0 条内容".to_string(), String::new()));
    }

    // -----------------------------------------------------------------------
    //  措辞规格
    // -----------------------------------------------------------------------

    /// 理由规格逐字：第 1 条写起点，第 k 条按证据三选一（同源 / 共享词 / 两者都有），
    /// 末尾一定有「相隔 N 分钟」；落地的理由 trim 后非空且没有首尾空白
    /// （数据库的条件 CHECK 只剥 ASCII 空格，构造侧先保证一遍 —— 三处钉住的第三处）。
    #[test]
    fn every_reason_is_explained_trimmed_and_chained_to_the_previous_item() {
        let items = [
            item(1, Some("iTerm2"), "redis 超时", at(14, 2)),
            item(2, Some("Chrome"), "redis 超时 重试", at(14, 22)),
            item(3, Some("Chrome"), "超时 重试", at(14, 42)),
        ];
        let drafts = build(&items, now());
        assert_eq!(drafts.len(), 1);
        assert_eq!(
            reason_of(&drafts[0], 1),
            "会话起点（iTerm2 · 14:02）",
            "第 1 条是起点，没有「与前一条」的证据可写"
        );
        assert_eq!(
            reason_of(&drafts[0], 2),
            "与前一条共享「redis」「超时」· 相隔 20 分钟",
            "跨应用：只有共享词这条证据"
        );
        assert_eq!(
            reason_of(&drafts[0], 3),
            "与前一条同源（Chrome）· 共享「超时」「重试」· 相隔 20 分钟",
            "同源 + 共享词：两条证据都写出来"
        );

        for member in &drafts[0].items {
            let reason = member.reason.as_deref().expect("agent 源每条都要有理由");
            assert!(
                !reason.trim().is_empty(),
                "理由 trim 后必须非空：{reason:?}"
            );
            assert_eq!(reason, reason.trim(), "理由不许带首尾空白：{reason:?}");
        }
        for member in drafts[0].items.iter().skip(1) {
            let reason = member.reason.as_deref().expect("agent 源每条都要有理由");
            assert!(reason.contains("相隔 "), "第 k 条要写清相隔多久：{reason}");
            assert!(
                reason.ends_with("分钟"),
                "相隔的措辞以「分钟」结尾：{reason}"
            );
        }
    }

    /// 标题 / 摘要规格逐字（计划的例子原样复刻）。
    #[test]
    fn titles_and_summaries_follow_the_wording_spec() {
        let items = [
            item(1, Some("iTerm2"), "redis 超时", at(14, 2)),
            item(2, Some("Chrome"), "redis 超时", at(14, 22)),
            item(3, Some("Chrome"), "redis 超时 重试", at(14, 42)),
        ];
        let drafts = build(&items, now());
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].title, "redis · 超时 · 3 条");
        assert_eq!(
            drafts[0].summary,
            "09-22 14:02–14:42（40 分钟）的 3 条内容；来源：iTerm2、Chrome；共享关键词：redis、超时。"
        );
    }

    /// 标题三级回退：共享词 → 唯一来源 → 条数；摘要里没有共享词就省略最后一段，
    /// 没有已知来源就不写来源段（不能写出空列表）。
    #[test]
    fn titles_fall_back_to_the_unique_source_then_to_the_plain_count() {
        let same_app = [
            item(1, Some("Chrome"), "alpha", at(14, 0)),
            item(2, Some("Chrome"), "beta", at(14, 5)),
        ];
        let drafts = build(&same_app, now());
        assert_eq!(drafts[0].title, "Chrome · 2 条");
        assert_eq!(
            drafts[0].summary,
            "09-22 14:00–14:05（5 分钟）的 2 条内容；来源：Chrome。"
        );

        // 双方都是未知来源：算「同源」（都是未知），但没有名字可写。
        let unknown = [
            item(3, None, "gamma", at(14, 0)),
            item(4, None, "delta", at(14, 5)),
        ];
        let drafts = build(&unknown, now());
        assert_eq!(drafts[0].title, "2 条内容", "未知来源没有名字，退回条数");
        assert_eq!(
            drafts[0].summary, "09-22 14:00–14:05（5 分钟）的 2 条内容。",
            "没有已知来源就不写来源段"
        );
        assert_eq!(
            reason_of(&drafts[0], 2),
            "与前一条同源（未知来源）· 相隔 5 分钟"
        );

        // 超过 3 个来源：写前 3 个 + 「等 N 个来源」。
        let many = [
            item(5, Some("iTerm2"), "redis", at(14, 0)),
            item(6, Some("Chrome"), "redis", at(14, 1)),
            item(7, Some("Safari"), "redis", at(14, 2)),
            item(8, Some("Notes"), "redis", at(14, 3)),
        ];
        let drafts = build(&many, now());
        assert_eq!(drafts[0].title, "redis · 4 条");
        assert_eq!(
            drafts[0].summary,
            "09-22 14:00–14:03（3 分钟）的 4 条内容；来源：iTerm2、Chrome、Safari 等 4 个来源；共享关键词：redis。"
        );
    }

    /// 跨天时两端都写 `MM-DD HH:MM`：`22:10–00:05` 会让人以为时间倒着走。
    #[test]
    fn cross_day_summaries_write_the_date_on_both_ends() {
        let items = [
            item(1, Some("Chrome"), "alpha", local(21, 23, 58)),
            item(2, Some("Chrome"), "beta", local(22, 0, 5)),
        ];
        let drafts = build(&items, now());
        assert_eq!(drafts.len(), 1, "跨天但相隔 7 分钟，仍是一段会话");
        assert_eq!(
            drafts[0].summary,
            "09-21 23:58–09-22 00:05（7 分钟）的 2 条内容；来源：Chrome。"
        );
    }

    /// 内容词只认文本：HTML 走 `searchable_text`（去标签 + 小写），图片 / 文件条目的
    /// `content_text` 是路径 —— 路径片段不是内容词，否则同一台机器上的两张截图永远
    /// 「共享 users / pictures」，理由会写出没有意义的句子。
    #[test]
    fn only_text_items_contribute_terms() {
        let html = [
            html_item(1, "Chrome", "<p>Redis 超时</p>", at(14, 0)),
            html_item(2, "Chrome", "<b>redis</b> 重试", at(14, 5)),
        ];
        let drafts = build(&html, now());
        assert_eq!(
            drafts[0].title, "redis · 2 条",
            "标签名不是内容词，正文里的 redis 是"
        );

        let pictures = [
            image_item(3, "Finder", "/Users/liang/Pictures/a.png", at(14, 0)),
            image_item(4, "Finder", "/Users/liang/Pictures/b.png", at(14, 5)),
        ];
        let drafts = build(&pictures, now());
        assert_eq!(
            drafts[0].title, "Finder · 2 条",
            "图片不贡献内容词，只剩唯一来源可写"
        );
        assert_eq!(
            drafts[0].summary,
            "09-22 14:00–14:05（5 分钟）的 2 条内容；来源：Finder。"
        );
        assert!(
            reason_of(&drafts[0], 2).contains("同源（Finder）"),
            "图片靠同源 + 时间成组：{}",
            reason_of(&drafts[0], 2)
        );
    }

    // -----------------------------------------------------------------------
    //  确定性
    // -----------------------------------------------------------------------

    /// 会话 id 由成员集合确定性派生（关键判断 4）：同一批内容 → 同一 id（重算前后是
    /// 同一行，界面不跳）；成员集合变了 → 另一条会话。
    #[test]
    fn session_ids_are_derived_from_the_member_set() {
        let items = [
            item(1, Some("iTerm2"), "redis 超时", at(14, 0)),
            item(2, Some("iTerm2"), "redis 超时", at(14, 5)),
        ];
        let once = build(&items, now());
        assert_eq!(once.len(), 1);
        assert!(once[0].id.starts_with("sess-"), "id 前缀：{}", once[0].id);
        assert_eq!(
            once[0].id.len(),
            37,
            "sess- + 32 位十六进制：{}",
            once[0].id
        );
        assert!(
            once[0].id[5..].chars().all(|ch| ch.is_ascii_hexdigit()),
            "哈希段是十六进制：{}",
            once[0].id
        );
        assert_eq!(
            build(&items, now() + Duration::hours(2))[0].id,
            once[0].id,
            "同一成员集合 → 同一 id（与 now 无关）"
        );

        let mut grown_items = items.to_vec();
        grown_items.push(item(3, Some("iTerm2"), "redis 超时", at(14, 10)));
        let grown = build(&grown_items, now());
        assert_eq!(grown.len(), 1);
        assert_ne!(
            grown[0].id, once[0].id,
            "多了成员就是另一条会话：旧案例在同一次重算里被清掉"
        );
        assert_eq!(grown[0].items.len(), 3);
    }

    /// 同一输入、同一时钟 → 标题 / 摘要 / 理由 / id 逐字相等（重算幂等的前提）；
    /// 措辞里不含「N 分钟前」这类相对时间 —— 它会随调用时刻变化。
    #[test]
    fn build_is_deterministic_for_the_same_input_and_clock() {
        let items = [
            item(1, Some("iTerm2"), "redis 超时", at(14, 2)),
            item(2, Some("Chrome"), "redis 超时", at(14, 22)),
        ];
        assert_eq!(
            snapshot(&build(&items, now())),
            snapshot(&build(&items, now())),
            "同一输入两次调用逐字相等"
        );
        let summary = &build(&items, now())[0].summary;
        assert!(
            !summary.contains('前'),
            "摘要只用绝对时间，不用相对措辞：{summary}"
        );
    }

    /// 常量钉子：单会话上限**就是**一次批量归纳的上限（关键判断 8）——
    /// 用户看到一条 21 条的会话、点「AI 归纳」必然撞 `ai_too_many_items`，
    /// 那说明分组本身就越界了，所以两个数只能是同一个常量。
    #[test]
    fn session_constants_match_the_contract() {
        assert_eq!(
            SESSION_MAX_ITEMS,
            crate::application::agent_service::MAX_AGENT_INPUT_ITEMS
        );
        assert_eq!(SESSION_MAX_ITEMS, 20);
        assert_eq!(SESSION_MAX_GAP_MINUTES, 30, "半小时是一段连续操作的粒度");
        assert_eq!(SESSION_MIN_ITEMS, 2, "一条不成组");
    }

    // -----------------------------------------------------------------------
    //  用户手动保存
    // -----------------------------------------------------------------------

    /// 用户显式保存：单组不切分（不过时间 / 来源门槛）、不给理由、
    /// 时间取组内最早 / 最晚的复制时刻。
    #[test]
    fn user_sessions_keep_every_selected_item_without_a_reason() {
        let items = [
            item(1, Some("iTerm2"), "alpha", at(9, 0)),
            item(2, Some("Chrome"), "beta", at(13, 30)),
            item(3, None, "gamma", at(14, 5)),
        ];
        let draft = build_user_session(&items, now());
        assert_eq!(draft.source, SessionSource::User);
        assert_eq!(
            member_ids(&draft),
            vec![
                item_id(1).to_string(),
                item_id(2).to_string(),
                item_id(3).to_string()
            ],
            "不切分，成员按复制时间升序"
        );
        assert!(
            draft.items.iter().all(|member| member.reason.is_none()),
            "用户自己知道为什么，不需要理由"
        );
        assert_eq!(
            draft
                .items
                .iter()
                .map(|member| member.position)
                .collect::<Vec<_>>(),
            vec![1, 2, 3],
            "position 从 1 开始"
        );
        assert_eq!(draft.created_at, items[0].last_copied_at);
        assert_eq!(draft.updated_at, items[2].last_copied_at);
        assert_eq!(
            draft.title, "3 条内容",
            "三种来源、没有共享词 → 只有条数可写"
        );
        assert_eq!(
            draft.summary,
            "09-22 09:00–14:05（305 分钟）的 3 条内容；来源：iTerm2、Chrome。"
        );
    }

    /// 用户显式选择不过时间门槛：未来的脏时间戳也不丢成员（静默丢掉他选中的人选
    /// 更糟），只把会话行的时间夹回「已经发生」的范围 —— 否则列表按
    /// `updated_at DESC` 会把这条会话永久钉在顶端。
    #[test]
    fn user_sessions_clamp_future_timestamps_without_dropping_members() {
        let items = [
            item(1, Some("Notes"), "delta", at(20, 0)),
            item(2, Some("Notes"), "epsilon", at(21, 0)),
        ];
        let draft = build_user_session(&items, now());
        assert_eq!(draft.items.len(), 2, "用户选了几条就存几条");
        assert_eq!(draft.created_at, now(), "来自未来的时间夹回 now");
        assert_eq!(draft.updated_at, now());
    }
}
