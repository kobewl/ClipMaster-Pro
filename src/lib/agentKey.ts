/** API Key 与端点配置的前端即时校验。后端有一份更严格且权威的实现。 */

/** 返回错误信息，null 表示通过。 */
export function validateAgentKey(raw: string): string | null {
  const key = raw.trim();
  if (key.length === 0) return "API Key 不能为空。";
  if (key.length < 8) return "API Key 太短，请确认复制完整。";
  if (/\s/.test(key)) return "API Key 不能包含空格或换行。";
  // eslint-disable-next-line no-control-regex
  if (/[^\x20-\x7E]/.test(key)) return "API Key 应只包含 ASCII 字符，请确认没有复制到中文标点。";
  return null;
}

/** 去掉首尾空白与零宽字符（从网页复制 Key 时常夹带，肉眼看不见但会导致认证失败）。 */
export function normalizeAgentKey(raw: string): string {
  return raw.replace(/[\u200B-\u200F\u202A-\u202E\u2060-\u2064\uFEFF]/g, "").trim();
}

/** 服务地址的前端预检。只做能立刻判断的错误，真正的规则在后端。 */
export function validateAgentBaseUrl(raw: string): string | null {
  const url = raw.trim().replace(/\/+$/, "");
  if (url.length === 0) return "服务地址不能为空。";
  if (!/^https?:\/\//i.test(url)) {
    return "地址要以 https:// 开头（本机服务可用 http://）。";
  }
  if (url.startsWith("http://")) {
    const host = url.slice("http://".length).split(/[/:?]/)[0].toLowerCase();
    const isLocal = host === "localhost" || host === "127.0.0.1" || host === "[::1]" || host === "::1";
    if (!isLocal) return "非本机地址必须使用 https://，否则 API Key 会明文传输。";
  }
  if (!/^https?:\/\/[^/?#\s]+/i.test(url)) return "地址里缺少主机名。";
  if (url.includes("?")) return "地址里不能带查询参数。";
  if (url.includes("#")) return "地址里不能带 # 片段。";
  if (/^https?:\/\/[^/@]*@/.test(url)) return "地址里不能包含用户名或密码。";
  return null;
}

/** 常用服务商，设置面板里做成一键填入。 */
export const AGENT_PRESETS = [
  { label: "DeepSeek", baseUrl: "https://api.deepseek.com", model: "deepseek-flash" },
  { label: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  { label: "本地 Ollama", baseUrl: "http://127.0.0.1:11434/v1", model: "llama3.1" },
] as const;
