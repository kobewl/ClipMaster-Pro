//! Planner 建议协议：模型输出的解析、四类动作白名单与丢弃规则（Phase 1 第 5 步）。
//!
//! 全部是**纯函数**：不碰数据库、不读设置、不发网络请求 —— 调用方
//! （`PlannerService`）负责取会话成员、调模型、把编号映射回条目，这里只回答
//! 「这一段模型输出里有几条能落地的建议」。
//!
//! **协议文本与解析器在同一个文件里**：模型被告知什么格式，解析器就认什么格式，
//! 改一处就是两套口径（`PLANNER_INSTRUCTION` 与 `parse` 必须一起改）。
//!
//! 每一行建议过六道校验，任何一道不过都**丢弃这一行**（不猜、不修正、不兜底成
//! 别的动作）：① JSON 能解析；② `action` 在白名单里；③ 每个编号都能映射到本次
//! 实际送出的 `[n]`；④ 动作与目标数的约束；⑤ `ai` 类的 `ai_action` 是已知 key；
//! ⑥ `reason` trim 后非空。

use serde::Deserialize;

use crate::application::agent_service::{AgentAction, MAX_AGENT_INPUT_ITEMS};
use crate::application::session_build::SESSION_MAX_ITEMS;
use crate::domain::model::PlannerSuggestionKind;

/// 白名单：四类有副作用的动作。
///
/// **唯一入口** —— 协议文本（`PLANNER_INSTRUCTION`）、解析器（`parse`）与测试
/// 都从它取；模型写出第五种动作词（`delete` / `清空` / 改设置）时，第一步就
/// 在这里被挡掉。删除 / 清空类动作**不入白名单**：本期不做撤销，
/// 而确认卡片必须能写出真实的「怎么退回去」（四类都可逆或零损害）。
pub const PLANNER_ACTION_KEYS: [&str; 4] = ["copy", "paste", "group", "ai"];

/// 一次建议最多几行。
///
/// 量级理由：一次会话最多 20 条成员，而人一次能读完并做决定的「下一步」不超过
/// 一只手 —— 5 是界面一屏内能读完的量，多了等于把决定权又丢回给用户。
pub const MAX_SUGGESTIONS: u64 = 5;

/// 模型明确表态「没有值得执行的下一步」的唯一字面量。
///
/// 判定是**整段 trim 后严格相等**（大小写敏感）：写成 `none` 说明模型没按协议
/// 输出，那就按「一段没法解析的输出」丢弃，不猜它的意图。
pub const NONE_SENTINEL: &str = "NONE";

/// 给模型的协议规格。
///
/// 与解析器 `parse` **同源**：这里告诉模型什么格式，解析器就认什么格式，
/// 改一处就是两套口径。选 JSON Lines 而不是 `动作|编号|理由` 的理由：
/// 理由是一句中文散文，里面完全可能出现任何分隔符，定长字段协议会被它打破。
pub const PLANNER_INSTRUCTION: &str = "\
在上面的内容块之外，给出「下一步可以做什么」的建议。
只依据给出的内容，不要假设你能查询网络或外部信息。

输出格式：每行一个 JSON 对象（JSON Lines）。不要输出 Markdown 代码围栏，不要输出解释性段落。
每行的字段：
- action：动作类型，只能是 copy、paste、group、ai 之一；
- targets：这条建议作用的编号数组，编号就是上面内容块的编号，例如 [2] 或 [1,3]；
- reason：一句话说清为什么这一步值得做（必填，不能为空）；
- ai_action：只有 ai 类才需要，指定交给 AI 的具体动作。

四类动作各自的含义与目标数限制：
- copy：把内容写回系统剪贴板，只能 1 条目标；
- paste：把内容粘贴到当前应用，只能 1 条目标；
- group：把这几条归入一个分组，1 到 20 条目标；
- ai：把这几条交给 AI 处理，1 到 20 条目标；ai_action 只认这五个：summarize、translate_zh、explain、extract_tasks、format_json（format_json 只对单条内容有意义，只能 1 条目标）。

最多输出 5 行。如果没有值得执行的下一步，只输出 NONE，不要输出别的行。
不要建议删除、清空历史或修改设置这类动作。";

/// 解析结果里的一条建议（编号已映射成 `sent` 里的 item_id）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSuggestion {
    pub kind: PlannerSuggestionKind,
    /// ai 类的子动作 key（非 ai 类一律 `None`，脏字段不保留）。
    pub ai_action: Option<String>,
    pub target_item_ids: Vec<String>,
    pub reason: String,
}

/// 一次解析的完整结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPlan {
    /// 活下来的建议，保持模型给出的顺序。
    pub items: Vec<ParsedSuggestion>,
    /// 被丢弃的行数（含超出 [`MAX_SUGGESTIONS`] 而截断掉的行）。
    pub dropped: u64,
    /// 模型整段只说了 [`NONE_SENTINEL`]：这是**合法**的空计划，不是解析失败。
    pub explicit_none: bool,
}

/// 协议里的一行原文。多余字段由 serde 默认忽略（模型多写一个字段不该让整行作废）。
#[derive(Deserialize)]
struct RawSuggestion {
    action: String,
    targets: Vec<i64>,
    reason: String,
    ai_action: Option<String>,
}

/// 把模型输出解析成建议列表。
///
/// `sent` 是「prompt 里实际送出的 `(编号, item_id)`」—— 模型只能指向它真看到过
/// 的条目，编号越界或凭空造 id 一律丢弃。输入刻意是切片而不是调用方的报告结构，
/// 这样本模块保持纯函数（不依赖 store、无 DB、无网络）。
pub fn parse(content: &str, sent: &[(usize, String)]) -> ParsedPlan {
    // 整段恰为哨兵：模型明确说「没有建议」。放在逐行解析之前，因为 `NONE`
    // 本身不是一个 JSON 对象，逐行走会把它当成一行解析失败。
    if content.trim() == NONE_SENTINEL {
        return ParsedPlan {
            items: Vec::new(),
            dropped: 0,
            explicit_none: true,
        };
    }

    let mut items: Vec<ParsedSuggestion> = Vec::new();
    let mut dropped: u64 = 0;

    for line in content.lines() {
        let line = line.trim();
        // 空行既不是建议也不是错误：模型在 JSON Lines 之间留空行很常见。
        if line.is_empty() {
            continue;
        }

        // 校验①：能解析成 JSON 对象。围栏、散文、半截 JSON 都算解析失败。
        let raw: RawSuggestion = match serde_json::from_str(line) {
            Ok(raw) => raw,
            Err(_) => {
                dropped += 1;
                continue;
            }
        };

        match validate(raw, sent) {
            Some(suggestion) => items.push(suggestion),
            None => dropped += 1,
        }
    }

    // 行数上限：按模型给出的顺序截断，多出来的如实计入丢弃。
    if items.len() as u64 > MAX_SUGGESTIONS {
        let overflow = items.len() as u64 - MAX_SUGGESTIONS;
        items.truncate(MAX_SUGGESTIONS as usize);
        dropped += overflow;
    }

    ParsedPlan {
        items,
        dropped,
        explicit_none: false,
    }
}

/// 六道校验里②到⑥的落地：任何一道不过都返回 `None`（调用方计入丢弃）。
///
/// 一律**丢弃而不修正**：猜一个「模型大概想做什么」再补上，等于把模型的口误
/// 变成一条用户可能误确认的副作用建议 —— 说不清就不产出。
fn validate(raw: RawSuggestion, sent: &[(usize, String)]) -> Option<ParsedSuggestion> {
    // 校验②：动作必须在白名单里，且解析器也认得（常量与 from_key 的集合相等
    // 由测试钉住）。`delete` / `清空` 这类词即使模型写出来也到不了用户面前。
    if !PLANNER_ACTION_KEYS.contains(&raw.action.as_str()) {
        return None;
    }
    let kind = PlannerSuggestionKind::from_key(&raw.action)?;

    // 校验⑥：理由 trim 后非空 —— 说不清为什么值得做的建议不产出。
    let reason = raw.reason.trim();
    if reason.is_empty() {
        return None;
    }

    // 校验③：每个编号都要映射到本次真正送出的 `[n]`。空数组同样是丢弃：
    // 没有目标的建议不是建议。
    if raw.targets.is_empty() {
        return None;
    }
    let mut target_item_ids = Vec::with_capacity(raw.targets.len());
    for number in &raw.targets {
        let (_, item_id) = sent.iter().find(|(index, _)| *index as i64 == *number)?;
        target_item_ids.push(item_id.clone());
    }

    // 校验④：动作与目标数的约束。copy / paste 是「写一份内容」的动作，两条
    // 目标等于写两次、谁赢不定，不是用户能预期的事；group 天然多目标；
    // ai 的上限与批量入口同一个数（一个会话永远塞得进一次调用）。
    let count = target_item_ids.len();
    match kind {
        PlannerSuggestionKind::Copy | PlannerSuggestionKind::Paste if count != 1 => return None,
        PlannerSuggestionKind::Group if count > SESSION_MAX_ITEMS => return None,
        PlannerSuggestionKind::Ai if count > MAX_AGENT_INPUT_ITEMS => return None,
        _ => {}
    }

    // 校验⑤：ai 类必须有已知的 ai_action；非 ai 类一律置 None，不保留脏字段。
    //
    // `trim` 只对 `ai_action` 做、不对 `action` 做，这是**刻意的**不对称：`action`
    // 是协议主键（白名单的索引），逐字比对 —— 容忍空白等于给白名单外的词留了
    // 改写空间；`ai_action` 是模型自由填的子字段，容忍它排版时多带的空白。
    // 两边各自的接受 / 拒绝性由 `ai_action_must_be_a_known_key` 末尾的断言钉住
    // （`" summarize "` 保留、`" copy "` 丢弃），别把它「顺手统一」成一边倒。
    let ai_action = match kind {
        PlannerSuggestionKind::Ai => {
            let key = raw.ai_action.as_deref().map(str::trim)?;
            let action = AgentAction::from_key(key)?;
            // format_json 对一组内容没有意义 —— 与 run_agent_action_batch 拦
            // ai_batch_unsupported_action 是同一条约束，两处都问 supports_batch()。
            if count > 1 && !action.supports_batch() {
                return None;
            }
            Some(key.to_string())
        }
        _ => None,
    };

    Some(ParsedSuggestion {
        kind,
        ai_action,
        target_item_ids,
        reason: reason.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agent_service::AgentAction;
    use crate::domain::model::PlannerSuggestionKind;

    /// 一次调用实际送出的 `(编号, item_id)` —— 编号与模型看到的 `[n]` 一一对应。
    fn sent(count: usize) -> Vec<(usize, String)> {
        (1..=count).map(|i| (i, format!("item-{i}"))).collect()
    }

    /// 从协议文本里抠出一串 `、` 分隔的字面量（起止标记之间的内容）。
    ///
    /// 为什么不能只用 `contains`：把 `explain` 改写成 `explain_zh` 时，原词仍是
    /// 新词的子串，「出现性」断言照样为真，而模型会照着写出解析器不认的 key。
    /// 抠出来做集合比较才钉得住「文本里那一串恰是这些」。
    fn listed_terms(start: &str, end: &str) -> Vec<&'static str> {
        let after = PLANNER_INSTRUCTION
            .split_once(start)
            .unwrap_or_else(|| panic!("协议文本里找不到起标记：{start}"))
            .1;
        let listed = after
            .split_once(end)
            .unwrap_or_else(|| panic!("协议文本里找不到止标记：{end}"))
            .0;
        listed.split('、').map(str::trim).collect()
    }

    /// 排序后比较：清单在文本里的先后不承载语义，集合相等才是契约。
    fn sorted(mut terms: Vec<&str>) -> Vec<&str> {
        terms.sort_unstable();
        terms
    }

    /// 抠出协议文本里两个锚点之间的一个整数。
    ///
    /// 不用 `contains(&n.to_string())` 的理由：单个数字太短，`5` 会撞上文本里
    /// 任何一处 `5`，改错地方也照样「包含」；锚点把上下文一起钉住，锚点本身
    /// 被改掉就直接 panic（起止锚点都含「1 到」时，下界也一并钉住）。
    fn number_between(start: &str, end: &str) -> u64 {
        let after = PLANNER_INSTRUCTION
            .split_once(start)
            .unwrap_or_else(|| panic!("协议文本里找不到起标记：{start}"))
            .1;
        let digits = after
            .split_once(end)
            .unwrap_or_else(|| panic!("协议文本里找不到止标记：{end}"))
            .0
            .trim();
        digits
            .parse()
            .unwrap_or_else(|_| panic!("协议文本里 {start}<{digits}>{end} 不是一个整数"))
    }

    #[test]
    fn parses_all_four_action_kinds_and_resolves_numbers_to_item_ids() {
        let content = concat!(
            r#"{"action":"copy","targets":[2],"reason":"这条命令是刚才排查的结论，可以直接复制回终端"}"#,
            "\n",
            r#"{"action":"paste","targets":[3],"reason":"把这条路径粘回终端里继续排查"}"#,
            "\n",
            r#"{"action":"group","targets":[1,3],"reason":"这几条同属一次排查，归到一个分组"}"#,
            "\n",
            r#"{"action":"ai","targets":[1,2],"ai_action":"summarize","reason":"把这次排查归纳成一段交接说明"}"#,
            "\n",
        );
        let plan = parse(content, &sent(3));

        assert_eq!(plan.items.len(), 4, "四类动作各一行，都该保留");
        assert_eq!(plan.dropped, 0);
        assert!(!plan.explicit_none);

        assert_eq!(plan.items[0].kind, PlannerSuggestionKind::Copy);
        assert_eq!(plan.items[1].kind, PlannerSuggestionKind::Paste);
        assert_eq!(plan.items[2].kind, PlannerSuggestionKind::Group);
        assert_eq!(plan.items[3].kind, PlannerSuggestionKind::Ai);

        // 目标存的是编号对应的 item_id，不是编号本身：`[n]` 只在这一次调用里有
        // 意义，出了 prompt 就作废。
        assert_eq!(plan.items[0].target_item_ids, vec!["item-2".to_string()]);
        assert_eq!(
            plan.items[2].target_item_ids,
            vec!["item-1".to_string(), "item-3".to_string()]
        );
        assert_eq!(plan.items[0].ai_action, None, "非 ai 类不该带子动作");
        assert_eq!(plan.items[3].ai_action.as_deref(), Some("summarize"));
        assert_eq!(
            plan.items[0].reason,
            "这条命令是刚才排查的结论，可以直接复制回终端"
        );
    }

    #[test]
    fn unknown_action_words_are_dropped_not_guessed() {
        // 删除 / 清空类词汇即使模型写出来，也进不了建议列表（白名单是唯一入口）。
        let content = concat!(
            r#"{"action":"delete","targets":[1],"reason":"r"}"#,
            "\n",
            r#"{"action":"清空","targets":[1],"reason":"r"}"#,
            "\n",
            r#"{"action":"rm -rf","targets":[1],"reason":"r"}"#,
            "\n",
            r#"{"action":"设置","targets":[1],"reason":"r"}"#,
            "\n",
        );
        let plan = parse(content, &sent(3));

        assert!(plan.items.is_empty(), "白名单四类之外一个都不许留");
        assert_eq!(plan.dropped, 4, "每一行都要如实计入丢弃数");
    }

    #[test]
    fn targets_must_resolve_to_sent_blocks() {
        // 模型不能指向它没看到的条目：编号越界、空数组、类型不对一律丢弃。
        let content = concat!(
            r#"{"action":"copy","targets":[0],"reason":"r"}"#,
            "\n",
            r#"{"action":"copy","targets":[99],"reason":"r"}"#,
            "\n",
            r#"{"action":"group","targets":[],"reason":"r"}"#,
            "\n",
            r#"{"action":"copy","targets":"a","reason":"r"}"#,
            "\n",
        );
        let plan = parse(content, &sent(3));

        assert!(plan.items.is_empty());
        assert_eq!(plan.dropped, 4);
    }

    #[test]
    fn single_target_actions_reject_multiple_targets() {
        let content = concat!(
            // 剪贴板一次只有一份内容：两条目标等于「写两次，谁赢不定」。
            r#"{"action":"copy","targets":[1,2],"reason":"r"}"#,
            "\n",
            r#"{"action":"paste","targets":[1,2],"reason":"r"}"#,
            "\n",
            // 分组天然是多目标动作；ai 的多条目标与批量入口同形。
            r#"{"action":"group","targets":[1,2,3,4,5],"reason":"r"}"#,
            "\n",
            r#"{"action":"ai","targets":[1,2],"ai_action":"summarize","reason":"r"}"#,
            "\n",
        );
        let plan = parse(content, &sent(5));

        assert_eq!(plan.items.len(), 2, "前两行丢弃，后两行保留");
        assert_eq!(plan.dropped, 2);
        assert_eq!(plan.items[0].kind, PlannerSuggestionKind::Group);
        assert_eq!(plan.items[1].kind, PlannerSuggestionKind::Ai);
    }

    #[test]
    fn format_json_only_accepts_one_target() {
        // format_json 对一组内容没有意义 —— 与 run_agent_action_batch 拦
        // ai_batch_unsupported_action 是同一条约束，两处都问 supports_batch()。
        let content = concat!(
            r#"{"action":"ai","targets":[1,2],"ai_action":"format_json","reason":"r"}"#,
            "\n",
            r#"{"action":"ai","targets":[1],"ai_action":"format_json","reason":"r"}"#,
            "\n",
        );
        let plan = parse(content, &sent(2));

        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.dropped, 1);
        assert_eq!(plan.items[0].target_item_ids, vec!["item-1".to_string()]);
    }

    #[test]
    fn ai_action_must_be_a_known_key() {
        let content = concat!(
            r#"{"action":"ai","targets":[1],"ai_action":"summarize","reason":"r"}"#,
            "\n",
            r#"{"action":"ai","targets":[1],"ai_action":"whatever","reason":"r"}"#,
            "\n",
            r#"{"action":"ai","targets":[1],"reason":"r"}"#,
            "\n",
            // 非 ai 类带上 ai_action 是脏字段：不保留，免得它被当成有效的子动作。
            r#"{"action":"copy","targets":[1],"ai_action":"summarize","reason":"r"}"#,
            "\n",
        );
        let plan = parse(content, &sent(1));

        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.dropped, 2);
        assert_eq!(plan.items[0].ai_action.as_deref(), Some("summarize"));
        assert_eq!(plan.items[1].kind, PlannerSuggestionKind::Copy);
        assert_eq!(
            plan.items[1].ai_action, None,
            "非 ai 类的 ai_action 一律置 None"
        );

        // 空白容忍度**刻意不对称**（注释在 validate 的校验⑤处）：
        // `ai_action` 是模型自由填的子字段，容忍排版空白 —— 带空白仍保留，
        // 且存进去的是 trim 后的形态（不把空白带进领域模型）。
        let padded = parse(
            r#"{"action":"ai","targets":[1],"ai_action":"  summarize  ","reason":"r"}"#,
            &sent(1),
        );
        assert_eq!(padded.items.len(), 1, "ai_action 带首尾空白照样认");
        assert_eq!(padded.dropped, 0);
        assert_eq!(padded.items[0].ai_action.as_deref(), Some("summarize"));
        assert_eq!(padded.items[0].kind, PlannerSuggestionKind::Ai);

        let inner = parse(
            r#"{"action":"ai","targets":[1],"ai_action":"summar ize","reason":"r"}"#,
            &sent(1),
        );
        assert!(
            inner.items.is_empty(),
            "空白只 trim 首尾，中间的空格照旧不认"
        );
        assert_eq!(inner.dropped, 1);

        // `action` 是协议主键（白名单的索引），逐字比对：带空白一律丢弃。
        // 容忍它等于给白名单外的词留了改写空间，两侧都断才有意义。
        let spaced_action = parse(
            r#"{"action":" copy ","targets":[1],"reason":"r"}"#,
            &sent(1),
        );
        assert!(
            spaced_action.items.is_empty(),
            "action 带空白必须丢弃（白名单逐字比对）"
        );
        assert_eq!(spaced_action.dropped, 1);
    }

    #[test]
    fn the_instruction_lists_exactly_the_ai_actions_the_parser_accepts() {
        // 硬写五个 key：将来加第 6 个 AgentAction 时，「指令里告诉模型的」与
        // 「解析器认的」分叉会被这条指出是哪一处漏改 —— 真表只有一个，
        // 就是 AgentAction 本身（这里直接问 from_key，不建镜像表）。
        const AI_ACTION_KEYS: [&str; 5] = [
            "summarize",
            "translate_zh",
            "explain",
            "extract_tasks",
            "format_json",
        ];

        for key in AI_ACTION_KEYS {
            assert!(
                PLANNER_INSTRUCTION.contains(key),
                "指令文本里漏了 AI 动作 {key}：模型不知道有这个动作"
            );
            assert!(
                AgentAction::from_key(key).is_some(),
                "解析器不认 {key}：指令里写了也落不了地"
            );

            // 第三处同源：拿指令里宣传的 key 造一行，必须真能解析出来。
            let content =
                format!(r#"{{"action":"ai","targets":[1],"ai_action":"{key}","reason":"r"}}"#);
            let plan = parse(&content, &sent(1));
            assert_eq!(plan.items.len(), 1, "key={key} 的那一行没解析出来");
            assert_eq!(plan.items[0].ai_action.as_deref(), Some(key));
        }

        assert!(
            AgentAction::from_key("whatever").is_none(),
            "认不出的 key 必须落空，不能糊一个默认动作"
        );

        // 第四处：把指令里那一串 key 抠出来做集合相等 —— 只断「出现过」挡不住
        // 把 explain 改写成 explain_zh 这类改法（原词仍是子串）。
        assert_eq!(
            sorted(listed_terms("只认这五个：", "（")),
            sorted(AI_ACTION_KEYS.to_vec()),
            "指令里列出的 AI 动作必须恰是解析器认的那五个"
        );
    }

    #[test]
    fn the_instruction_limits_match_the_constants_the_parser_enforces() {
        // 指令里写死的每个数字都必须与解析器真正执行的常量同源：不改这里就改
        // 文本，模型会照着解析器不认的上限输出，`dropped` 的诚实性随之失效 ——
        // 等于协议自己骗模型。锚点把上下文一起钉住（锚点含「1 到」则下界也被钉住，
        // 锚点消失直接 panic）。
        assert_eq!(
            number_between("最多输出 ", " 行"),
            MAX_SUGGESTIONS,
            "指令的行数上限必须等于 MAX_SUGGESTIONS"
        );

        assert_eq!(
            number_between("copy：把内容写回系统剪贴板，只能 ", " 条目标"),
            1,
            "copy 是单目标动作（解析器按 count != 1 丢弃）"
        );
        assert_eq!(
            number_between("paste：把内容粘贴到当前应用，只能 ", " 条目标"),
            1,
            "paste 同为单目标动作"
        );

        assert_eq!(
            number_between("group：把这几条归入一个分组，1 到 ", " 条目标"),
            SESSION_MAX_ITEMS as u64,
            "指令的 group 目标数上限必须等于解析器用的 SESSION_MAX_ITEMS"
        );
        assert_eq!(
            number_between("ai：把这几条交给 AI 处理，1 到 ", " 条目标"),
            MAX_AGENT_INPUT_ITEMS as u64,
            "指令的 ai 目标数上限必须等于解析器用的 MAX_AGENT_INPUT_ITEMS"
        );
    }

    #[test]
    fn empty_or_missing_reason_is_dropped() {
        // 说不清为什么值得做的建议不产出 —— 与 agent 源会话成员的 reason 义务同一条原则。
        let content = concat!(
            r#"{"action":"copy","targets":[1],"reason":""}"#,
            "\n",
            r#"{"action":"copy","targets":[1],"reason":"   "}"#,
            "\n",
            r#"{"action":"copy","targets":[1]}"#,
            "\n",
        );
        let plan = parse(content, &sent(1));

        assert!(plan.items.is_empty());
        assert_eq!(plan.dropped, 3);
    }

    #[test]
    fn prose_fences_and_broken_json_count_as_dropped() {
        let content = concat!(
            // 代码围栏本身不是建议行（模型爱加，加了就丢）。
            "```json",
            "\n",
            // 散文行。
            "我建议你把第二条复制一下，它可能是刚才排查的答案。",
            "\n",
            // 半截 JSON（模型被截断）。
            r#"{"action":"copy","targets":[1],"reason":"没写完"#,
            "\n",
            // 多余字段：忽略、建议保留（协议只认自己那几个字段）。
            r#"{"action":"copy","targets":[1],"reason":"带多余字段","extra":true}"#,
            "\n",
        );
        let plan = parse(content, &sent(1));

        assert_eq!(plan.items.len(), 1, "只有带多余字段那一行是合法建议");
        assert_eq!(plan.items[0].reason, "带多余字段");
        assert_eq!(plan.dropped, 3);
    }

    #[test]
    fn none_sentinel_means_a_legitimate_empty_plan() {
        assert_eq!(NONE_SENTINEL, "NONE");
        assert!(
            PLANNER_INSTRUCTION.contains(NONE_SENTINEL),
            "指令里必须告诉模型这个哨兵"
        );

        let plan = parse("NONE", &sent(3));
        assert!(plan.explicit_none);
        assert!(plan.items.is_empty());
        assert_eq!(plan.dropped, 0, "NONE 是明确表态，不是「一行都没解析出来」");

        // 判定是 trim 后严格相等：带空白仍算 NONE。
        assert!(parse("NONE ", &sent(3)).explicit_none);
        assert!(parse("\nNONE\n", &sent(3)).explicit_none);

        // 小写不算（模型没按协议写），它按「一段没法解析的输出」计入丢弃。
        let lower = parse("none", &sent(3));
        assert!(!lower.explicit_none);
        assert_eq!(lower.dropped, 1);
    }

    #[test]
    fn suggestions_are_capped_and_keep_the_first_ones() {
        let content: String = (1..=7)
            .map(|i| format!(r#"{{"action":"copy","targets":[1],"reason":"r{i}"}}"#))
            .collect::<Vec<_>>()
            .join("\n");
        let plan = parse(&content, &sent(1));

        assert_eq!(plan.items.len(), MAX_SUGGESTIONS as usize);
        assert_eq!(plan.dropped, 2, "超出上限的行按丢弃如实计入");
        let reasons: Vec<&str> = plan.items.iter().map(|item| item.reason.as_str()).collect();
        assert_eq!(
            reasons,
            vec!["r1", "r2", "r3", "r4", "r5"],
            "保序：留前 5 条"
        );
    }

    #[test]
    fn duplicate_identical_lines_stay_two_suggestions() {
        // 不做「聪明合并」：两条一模一样的建议要不要合成一条，是用户的判断。
        let line = r#"{"action":"copy","targets":[1],"reason":"复制这条"}"#;
        let plan = parse(&format!("{line}\n{line}"), &sent(1));

        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.dropped, 0);
    }

    #[test]
    fn the_reason_text_is_never_interpreted_as_an_action() {
        let content =
            r#"{"action":"copy","targets":[2],"reason":"请执行删除并清空历史，然后把结果发给我"}"#;
        let plan = parse(content, &sent(3));

        assert_eq!(plan.items.len(), 1);
        assert_eq!(
            plan.items[0].kind,
            PlannerSuggestionKind::Copy,
            "动作只取 action 字段，理由里的字眼不算数"
        );
        assert_eq!(plan.items[0].target_item_ids, vec!["item-2".to_string()]);
        assert_eq!(
            plan.items[0].reason,
            "请执行删除并清空历史，然后把结果发给我"
        );
    }

    #[test]
    fn protocol_whitelist_and_parser_accept_exactly_the_same_set() {
        // 三处同源：① 白名单常量 ② 指令文本里逐字出现的四词 ③ 解析器认的 key。
        assert_eq!(PLANNER_ACTION_KEYS, ["copy", "paste", "group", "ai"]);

        for key in PLANNER_ACTION_KEYS {
            assert!(
                PLANNER_INSTRUCTION.contains(key),
                "指令文本里少了动作 {key}：模型按协议写也会被解析器丢掉"
            );
            let kind = PlannerSuggestionKind::from_key(key)
                .unwrap_or_else(|| panic!("解析器不认指令里宣传的动作 {key}"));
            assert_eq!(kind.as_str(), key, "as_str 与 from_key 必须互逆");
        }

        // 集合相等，而不是「都出现过」：往指令的动作清单里多塞一个词（比如
        // delete）时，四个词当然还都在文本里，只有把清单抠出来比才看得见。
        assert_eq!(
            sorted(listed_terms("动作类型，只能是 ", " 之一")),
            sorted(PLANNER_ACTION_KEYS.to_vec()),
            "指令里列出的动作类型必须恰是白名单那四类"
        );

        // 白名单外的词一个都不认（含删除类与带空白的写法）。
        for key in ["delete", "clear", "copy "] {
            assert!(
                PlannerSuggestionKind::from_key(key).is_none(),
                "{key} 不该被认成动作"
            );
        }
    }
}
