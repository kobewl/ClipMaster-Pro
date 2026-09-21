import { useEffect, useState } from "react";
import type { AgentResult, AgentRun } from "@/types/clipboard";
import { formatRunDuration } from "@/types/clipboard";
import { commands } from "@/lib/commands";

interface Props {
  result: AgentResult;
  /** 复制按钮的回调（由调用方决定复制到哪儿）。 */
  onCopy: () => void;
  /** 复制成功后的显示态由调用方给，避免两处各记一份。 */
  copied: boolean;
}

/**
 * AI 结果卡片。单条与多条目共用。
 *
 * 抽成组件的理由不只是省代码：**多条目时"这条结论用了哪几条"是必须交代的**，
 * 单条时"用了哪一条"同样是。两处各写一份渲染，迟早会有一处忘了显示截断提示，
 * 而用户看到的就是一个看起来完全正常、实际基于残缺内容的结论。
 */
export function AgentResultCard({ result, onCopy, copied }: Props) {
  const [runDetail, setRunDetail] = useState<AgentRun | null>(null);
  const [loading, setLoading] = useState(false);

  // 换了结果就收起上一次读到的调用记录。不靠调用方传 key 来重挂载 ——
  // 那要求每个用到这张卡片的地方都记得写 key，漏一个就会出现
  // "新结果下面挂着上一次的调用记录"这种很难发现的数据串台。
  useEffect(() => {
    setRunDetail(null);
    setLoading(false);
  }, [result.request_id]);

  // 清单在两种情况下都要显示：
  // - 多条输入：这是"结论由哪几条支撑"的唯一交代；
  // - 单条但有值得说的情况（被截断 / 含疑似指令）：这种情况下不显示，
  //   用户就会以为模型看的是完整内容。
  const showInputList =
    result.inputs.length > 1 ||
    result.inputs.some((input) => input.truncated || input.suspicious);

  async function toggleDetail() {
    if (runDetail !== null) {
      setRunDetail(null);
      return;
    }
    if (loading) return;
    setLoading(true);
    try {
      setRunDetail(await commands.getAgentRun(result.request_id));
    } catch {
      setRunDetail(null);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="agent-result">
      <div className="agent-result__meta">
        <strong>{result.title}</strong>
        <span>
          {result.provider} · {result.model}
        </span>
      </div>

      {/* 被丢弃的条目必须显眼地说出来：结果看起来一切正常，
          但其实是基于用户选的一部分得出的。 */}
      {result.dropped_item_ids.length > 0 && (
        <p className="agent-result__warning" role="alert">
          ⚠ 有 {result.dropped_item_ids.length} 条内容因超出上限未参与本次处理（已优先保留最近的记录）。
        </p>
      )}

      <pre>{result.content}</pre>

      {/* 每条输入的处理情况：让用户能核对结论的依据是哪几条。
          多条目时这是结果的一部分，不是可选的附加信息。 */}
      {showInputList && (
        <ul className="agent-source-list">
          {result.inputs.map((input) => (
            <li key={input.item_id} className="agent-source-list__row">
              <span className="agent-source-list__index">[{input.index}]</span>
              <span className="agent-source-list__source">{input.source}</span>
              <span className="agent-source-list__size">
                {input.truncated
                  ? `${input.used_chars} / ${input.full_chars} 字 · 已截断`
                  : `${input.full_chars} 字`}
              </span>
              {input.suspicious && (
                <span
                  className="agent-source-list__flag"
                  title="这条内容里有「忽略之前的指令」这类句式，已被当作纯数据、不影响任务"
                >
                  含疑似指令
                </span>
              )}
            </li>
          ))}
        </ul>
      )}

      <div className="agent-result__foot">
        <button type="button" onClick={onCopy} className="button button--secondary button--compact">
          {copied ? "已复制 ✓" : "复制结果"}
        </button>
        <button
          type="button"
          className="agent-panel__link"
          aria-expanded={runDetail !== null}
          onClick={() => void toggleDetail()}
        >
          {loading ? "读取中…" : runDetail !== null ? "收起来源" : "查看调用记录"}
        </button>
      </div>

      {runDetail !== null && (
        <dl className="agent-result__detail">
          <dt>输入</dt>
          <dd>
            {runDetail.input_item_ids.length} 条记录 · {runDetail.input_chars} 字
          </dd>
          <dt>服务</dt>
          <dd>
            {runDetail.provider ?? "未发出请求"} · {runDetail.model ?? "—"}
          </dd>
          <dt>耗时</dt>
          <dd>
            {formatRunDuration(runDetail.duration_ms)}
            {runDetail.output_chars !== null && <> · 输出 {runDetail.output_chars} 字</>}
          </dd>
          <dt>请求号</dt>
          {/* 完整显示，不做缩写：这个号就是审计行的主键，用户要能直接拿它去查。
              缩写看着整齐，但会让"界面上这个号 = 库里那一行"这个承诺落空。 */}
          <dd className="agent-result__id">{runDetail.id}</dd>
        </dl>
      )}
    </div>
  );
}
