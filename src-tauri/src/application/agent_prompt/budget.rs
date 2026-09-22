//! 预算分配：把总预算按**水位填充**分给各条内容，并裁决截断与丢弃。

use super::AgentInput;

/// 一次请求发给模型的全部输入预算（Unicode 字符数）。
///
/// 沿用 Phase 0 的单条上限：单条时是一条 12000，多条目时是所有条合计 12000。
pub const TOTAL_BUDGET_CHARS: usize = 12_000;

/// 单条内容的上限。超出就截断并标记 —— 一条超长内容不该挤掉其它条目，
/// 否则用户选了 5 条却只得到 1 条的处理结果。
///
/// **只有一条内容时这个上限不生效**，那条可以吃满总预算：这个上限的目的是
/// 防止某条挤占别人的份额，只有一条时没有"别人"可挤；收紧会让用户手上原本
/// 能处理的长文突然开始被截断。见 `tests::a_single_item_uses_the_whole_budget`。
pub const PER_ITEM_MAX_CHARS: usize = 4_000;

/// 分配到的额度低于这个数就不给模型了。
///
/// 给模型一个 30 字的碎片，比不给更糟：它会被当成完整内容参与归纳，
/// 结论却是基于残缺信息得出的，而用户看不出来。
pub const MIN_USEFUL_CHARS: usize = 200;

/// 截断标记。放在内容末尾，让模型知道"这里被切过"。
const TRUNCATION_MARKER: &str = "\n…[已截断]";

/// 单条内容的分配结果（内部用）。
pub(crate) struct Allocation {
    /// 这条能拿到多少字符。
    pub(crate) quota: usize,
    /// 因为预算不够被整条丢弃。
    pub(crate) dropped: bool,
}

/// 把总预算分给各条内容。**水位填充**：均分 → 装得下的拿走自己的份、
/// 把富余还给池子 → 剩下的重新均分，直到没有人能被完全装下。
///
/// `needs` 是**转义之后**的长度，也就是真正会发出去的量。用原文长度会让预算
/// 悄悄超支：内容里每出现一次 `</clip>` 就会多出 1 个字符的转义开销。
///
/// 排序规则是**最近优先**：预算不够时先牺牲旧的那条，丢弃也只丢最旧的。
pub(crate) fn allocate(inputs: &[AgentInput], needs: &[usize]) -> Vec<Allocation> {
    let n = inputs.len();

    // 按最近优先给出处理顺序；结果数组仍按传入顺序对齐，方便上层组装。
    let mut priority: Vec<usize> = (0..n).collect();
    priority.sort_by(|&a, &b| inputs[b].last_copied_at.cmp(&inputs[a].last_copied_at));

    let mut result: Vec<Allocation> = (0..n)
        .map(|_| Allocation {
            quota: 0,
            dropped: true,
        })
        .collect();

    // 条数上限：预算除以最小可用额度。超出的（最旧的）整条丢弃。
    // 这条保证下面的均分永远 >= MIN_USEFUL_CHARS，不会给模型喂碎片。
    let max_items = (TOTAL_BUDGET_CHARS / MIN_USEFUL_CHARS).max(1);
    let alive: Vec<usize> = priority.iter().copied().take(max_items).collect();
    for &index in priority.iter().skip(max_items) {
        result[index].dropped = true;
    }

    let mut remaining = TOTAL_BUDGET_CHARS;
    let mut active: Vec<usize> = alive;

    while !active.is_empty() {
        // 单条上限取「硬上限」与「均分份额」里较大的那个：生硬卡硬上限会浪费预算
        // （两条各 8000 字时只用掉 8000），而"防止挤占别人份额"在均分份额里已经达成。
        let cap = PER_ITEM_MAX_CHARS.max(TOTAL_BUDGET_CHARS / active.len());
        let share = (remaining / active.len()).min(cap);
        if share == 0 {
            break;
        }

        // 装得下的：给它刚好需要的量，富余回到池子里。
        let satisfied: Vec<usize> = active
            .iter()
            .copied()
            .filter(|&i| needs[i] <= share)
            .collect();

        if satisfied.is_empty() {
            // 剩下的人都超份额：按份额截断，到此为止。
            for &index in &active {
                result[index].quota = share;
                result[index].dropped = false;
            }
            break;
        }

        for &index in &satisfied {
            result[index].quota = needs[index];
            result[index].dropped = false;
            remaining -= needs[index];
        }
        active.retain(|index| !satisfied.contains(index));
    }

    result
}

/// 按预算裁一段文本，末尾加截断标记。
///
/// **标记占用的字符从配额里扣**，让最终长度正好等于配额 —— 否则每条截断的
/// 内容都会超出配额几个字符，条数一多，总预算就不准了。
pub(crate) fn take_chars(text: &str, quota: usize, truncated: bool) -> String {
    if !truncated {
        return text.to_string();
    }
    let marker_len = TRUNCATION_MARKER.chars().count();
    let keep = quota.saturating_sub(marker_len);
    let mut out: String = text.chars().take(keep).collect();
    out.push_str(TRUNCATION_MARKER);
    out
}
