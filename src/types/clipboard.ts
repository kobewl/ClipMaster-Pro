export type ContentType = "text" | "image" | "html" | "files";

export interface ClipboardItem {
  id: string;
  content_type: ContentType;
  content_text: string;
  preview: string;
  group_id: string | null;
  created_at: string;
  updated_at: string;
  last_copied_at: string;
  source_app: string | null;
  source_url: string | null;
  /**
   * 这条命中了查询里的哪些词（词序同查询词序）。
   *
   * 由后端在**非空查询**时装配；空 query（浏览列表）时字段整个不下发，
   * 所以这里是可选的 —— 有值才画「命中 N/M 词」，浏览列表零残留。
   */
  matched_terms?: string[];
}

export interface ClipGroup {
  id: string;
  name: string;
  color: string;
  sort_order: number;
  item_count: number;
}

export interface ListQuery {
  group_id: string | null;
  search: string | null;
  /** 按内容类型筛选："text" | "image" | "html" | "files"。null = 全部。 */
  content_type: string | null;
  /** 预设时间档："today" | "week" | "month"。null = 全部时间。 */
  time_range: string | null;
  limit: number;
  offset: number;
}

export interface ListResult {
  items: ClipboardItem[];
  total: number;
  /**
   * 本次查询走了放宽召回时，**没有任何保留条目命中**的查询词（横幅用）。
   *
   * 三态要分清，别用 `?? []` 抹平：
   * - `undefined`：没走放宽（严格路径命中，或空 query）= 不画横幅；
   * - `[]`：走了放宽但没有词完全落空 = 只画横幅前半句；
   * - 非空：放宽把这几个词筛掉了 = 横幅追加「本次被筛除的词：…」。
   */
  relaxed_dropped?: string[];
}

export interface AppSettings {
  max_history: number;
  retention_days: number;
  capture_enabled: boolean;
  shortcut: string;
  /** `system` | `light` | `dark` */
  theme: string;
}

export interface CommandError {
  code: string;
  message: string;
  retryable: boolean;
  details?: unknown;
}

/** 检查更新的结果（后端 check_for_updates 命令）。 */
export interface UpdateStatus {
  current_version: string;
  update_available: boolean;
  latest_version: string | null;
  notes: string | null;
  /** 最新 release 的 GitHub 页面地址，「前往下载」直接打开它。 */
  release_url: string | null;
}

export type AgentAction =
  | "summarize"
  | "translate_zh"
  | "explain"
  | "extract_tasks"
  | "format_json";

/** 一条输入在这次调用里的实际处理情况。编号与结果正文里的 [n] 引用对应。 */
export interface AgentInputReport {
  index: number;
  item_id: string;
  source: string;
  /** 原文长度（截断前）。 */
  full_chars: number;
  /** 实际发给模型的长度。 */
  used_chars: number;
  truncated: boolean;
  /** 内容里有疑似 prompt injection 的句式，已被标记为纯数据。 */
  suspicious: boolean;
}

export interface AgentResult {
  request_id: string;
  action: string;
  title: string;
  content: string;
  provider: string;
  model: string;
  source_item_ids: string[];
  /** 每条输入的处理情况。 */
  inputs: AgentInputReport[];
  /** 因为超出条数上限而整条没送进模型的记录（最旧的先丢）。 */
  dropped_item_ids: string[];
}

/** 一次最多处理多少条记录（与后端 MAX_AGENT_INPUT_ITEMS 保持一致）。 */
export const MAX_AGENT_INPUT_ITEMS = 20;

/** 这个动作能不能对一组内容运行。格式化 JSON 只对单条有意义。 */
export function supportsBatch(action: AgentAction): boolean {
  return action !== "format_json";
}

/** Key 的来源：系统钥匙串（用户填的）或开发期环境变量。 */
export type AgentKeySource = "keychain" | "env";

/** 设置面板用的模型配置快照。**没有密钥本体**，只有掩码提示。 */
export interface AgentConfigInfo {
  configured: boolean;
  /** 形如 "••••••••abcd"，没有配置时为 null。 */
  key_hint: string | null;
  key_source: AgentKeySource;
  base_url: string;
  model: string;
  /** 地址是用户自己设的（false = 正在用内置默认值）。 */
  base_url_is_custom: boolean;
  /** 界面上显示的服务名（默认地址是 "DeepSeek"，自定义地址是真实主机名）。 */
  provider_label: string;
}

/**
 * 一条 AI 使用记录。**没有 prompt 和响应正文** —— 那些内容就在剪贴板历史里，
 * 审计再存一份等于把隐私面翻倍。
 */
export interface AgentRun {
  id: string;
  created_at: string;
  /** 机器可读的动作名（汇总统计用）。 */
  action: string;
  /** 界面直接显示的动作名。 */
  action_label: string;
  /** 发往的服务；null = 在本地就被拦下，什么都没发出去。 */
  provider: string | null;
  model: string | null;
  input_item_ids: string[];
  input_chars: number;
  status: "ok" | "error";
  error_code: string | null;
  duration_ms: number;
  output_chars: number | null;
}

/** 调用耗时：秒级以下给毫秒，之上给一位小数的秒。 */
export function formatRunDuration(ms: number): string {
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
}

// ---------------------------------------------------------------------------
//  Flow 会话（Phase 1 第 3 步）
// ---------------------------------------------------------------------------

/** 会话来源：`user` 是用户亲手存的，`agent` 是本地按时间/来源/词算出来的。 */
export type AgentSessionSource = "user" | "agent";

/** 列表里的一条会话（没有成员，只有概览）。 */
export interface AgentSessionSummary {
  id: string;
  source: AgentSessionSource;
  title: string;
  /** 一句话摘要：本地拼接的时段 / 来源 / 共享词，不含模型产出。 */
  summary: string;
  created_at: string;
  updated_at: string;
  item_count: number;
}

/** 详情里的一条成员。 */
export interface AgentSessionMember {
  item: ClipboardItem;
  /** 会话内编号，从 1 开始 —— 界面上的编号就是库里的编号（关键判断 12）。 */
  position: number;
  /**
   * 这条为什么在会话里。`null` = 用户手动保存（他为什么把它们放在一起，
   * 不需要机器解释），**不是**「理由缺失」—— 三态里 null 是有意义的一态。
   */
  reason: string | null;
}

/**
 * 会话详情。
 *
 * **没有 `item_count`**：详情的条数就是 `members.length`，多一个字段就多一处
 * 可能与成员列表对不上的地方（与后端 DTO 的形状一致）。
 */
export interface AgentSessionDetail {
  id: string;
  source: AgentSessionSource;
  title: string;
  summary: string;
  created_at: string;
  updated_at: string;
  members: AgentSessionMember[];
}

/** 一键清除 AI 派生数据的结果：会话与使用记录各自删掉了多少条。 */
export interface ClearDerivedDataResult {
  sessions: number;
  runs: number;
}

/**
 * 一个会话最多多少条（与后端 `SESSION_MAX_ITEMS` 保持一致）。
 *
 * **同源风险**：这里与 `session_build::SESSION_MAX_ITEMS` 是两份值，改一边必须
 * 改另一边 —— 后端那个 20 直接引用 `MAX_AGENT_INPUT_ITEMS`（同一次批量归纳的
 * 上限），所以这个数也只能跟着它一起动。照 `MAX_AGENT_INPUT_ITEMS` 的先例。
 */
export const MAX_SESSION_ITEMS = 20;

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}

export const GROUP_COLORS = [
  "#EF4444", "#F97316", "#F59E0B", "#22C55E",
  "#06B6D4", "#3B82F6", "#8B5CF6", "#EC4899",
  "#6B7280",
] as const;

// ---------------------------------------------------------------------------
//  查询分词（后端 query_parse::parse_query 的前端镜像）
// ---------------------------------------------------------------------------

/**
 * 命中词 chip 的分母来源。
 *
 * **为什么不问后端要 `query_term_count`**：DTO 加字段要动 Rust 侧契约（本任务
 * 范围只在前端），而分母必须与分子 `matched_terms` 用**同一套分词规则** ——
 * 对不上就会出现「命中 2/4 词」这种把用户绕晕的数字。
 *
 * **同源风险（重要）**：下面三张表与 `src-tauri/src/application/query_parse.rs`
 * 是**两份拷贝**，改一边必须改另一边，`clipboard.test.ts` 里的用例逐条照抄了
 * Rust 单测做钉子。真要根治，应在 DTO 上加 `query_term_count` 由后端下发布。
 */
const QUERY_STOP_WORDS: ReadonlySet<string> = new Set([
  "的", "了", "吗", "呢", "吧", "啊", "一下", "找", "找找", "搜索", "关于",
  "请问", "帮我", "如何", "怎么", "怎样", "为什么", "是什么", "怎么做",
]);

/**
 * 检索前缀，按长度从长到短（「怎么做」不能被「怎么」先吃掉一半）。
 *
 * `是什么` / `什么是` 是词序相反的一对疑问短语，**必须同时出现在这一表与
 * Rust 侧的 `QUERY_PREFIXES` 里**：只补一边，「什么是 mysql 索引」在一侧算
 * 2 个词、另一侧算 3 个词，两处分词结果不同源 —— chip 的分母会对不上后端的
 * 分子，放宽阈值也跟着差一档（确定性差异，不是边缘码点那种版本差异）。
 */
const QUERY_PREFIXES: readonly string[] = [
  "为什么", "是什么", "什么是", "怎么做", "请问", "帮我", "如何", "怎么", "怎样", "搜索", "关于", "找找",
];

/** 词内符号白名单：技术词里的这些符号是内容，不能当首尾标点剥掉。 */
const CONTENT_SYMBOLS: ReadonlySet<string> = new Set([
  "+", "#", ".", "_", "-", "/", "\\", "~", "&", "=", "@", "$", "%", "*", "^", "|", "`",
]);

/**
 * 与 Rust `char::is_whitespace()` 对齐的空白集合（`White_Space` 属性），
 * **不是** JS 的 `\s`：两者差在两端 ——
 * `\s` 多含 U+FEFF（零宽不换行空格），少含 U+0085（NEL）。
 * 用 `\s` 会在「只由零宽空格组成的查询」上把 JS 分词算成 0 个词、
 * 而 Rust 算成 1 个（进而在不该出 chip 时出 chip）。
 */
const RUST_WHITESPACE = /[ \t\n\v\f\r\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+/;

/**
 * 与 Rust `char::is_alphanumeric()` 对齐：`Alphabetic || Nd || Nl || No`。
 *
 * **为什么不是 `\p{L}\p{N}`**：二者实测差 6167 个码点，全部集中在
 * Mn(927) / Mc(438) / So(130) 与版本差 Cn(4672) —— 比如天城文元音符
 * `ा`(U+093E, Mc) 与带圈字母 `Ⓜ`(U+24C2, So) 在 Rust 侧算"内容"，
 * 用 `\p{L}\p{N}` 却会被当标点剥掉，直接导致 chip 的分子分母不同源。
 * `\p{Alphabetic}` 恰好覆盖 Sn 里那些 Other_Alphabetic 的记号，
 * 换成它之后上述**已分配字符**的差异归零。
 *
 * 残留差异只有 4672 个「JS 引擎的 Unicode 表尚未分配、Rust 已分配」的新码点
 * （版本差，随引擎升级自然收敛），方向恒为 Rust 真 / JS 假 ——
 * 影响是 JS 会把它当首尾标点剥掉，而保底回退仍保住词数（见 `parseQueryTerms`）。
 */
function isAlphanumeric(ch: string): boolean {
  return /[\p{Alphabetic}\p{N}]/u.test(ch);
}

/** 去掉词首尾的标点，词内符号保留（`min-width` / `c++` / `node.js`）。 */
function stripBoundaryPunct(token: string): string {
  const chars = Array.from(token);
  let start = 0;
  let end = chars.length;
  while (start < end && !isAlphanumeric(chars[start]) && !CONTENT_SYMBOLS.has(chars[start])) start += 1;
  while (end > start && !isAlphanumeric(chars[end - 1]) && !CONTENT_SYMBOLS.has(chars[end - 1])) end -= 1;
  return chars.slice(start, end).join("");
}

/** 反复剥掉词首的检索前缀（「请问怎么做redis」→「redis」）。 */
function stripQueryPrefixes(token: string): string {
  let rest = token;
  for (;;) {
    const hit = QUERY_PREFIXES.find((prefix) => rest.startsWith(prefix));
    if (hit === undefined) return rest;
    rest = rest.slice(hit.length);
  }
}

/**
 * 把原始查询串解析成检索词，规则与后端 `query_parse::parse_query` 完全一致：
 * 按空白分词 → 去首尾标点 → 小写 → 剥检索前缀 → 剔停用词 → 去重（保序）。
 *
 * 全被剔空时回退到「归一后的原始非空词」（纯符号词如 `😀` 保留原文），
 * 与后端的保底规则一致 —— 否则 chip 的分母会比分子所在的词集少。
 */
export function parseQueryTerms(raw: string): string[] {
  const tokens: string[] = [];
  const push = (token: string) => {
    if (!tokens.includes(token)) tokens.push(token);
  };

  for (const rawToken of raw.split(RUST_WHITESPACE).filter((t) => t.length > 0)) {
    const token = stripBoundaryPunct(rawToken).toLowerCase();
    if (token.length === 0) continue;
    const stripped = stripQueryPrefixes(token);
    if (stripped.length === 0 || QUERY_STOP_WORDS.has(stripped)) continue;
    push(stripped);
  }
  if (tokens.length === 0) {
    for (const rawToken of raw.split(RUST_WHITESPACE).filter((t) => t.length > 0)) {
      const token = stripBoundaryPunct(rawToken).toLowerCase();
      push(token.length === 0 ? rawToken : token);
    }
  }
  return tokens;
}
