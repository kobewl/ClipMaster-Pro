/**
 * 请求号生成器。
 *
 * 号由前端生成（后端只做形状校验），取消、关窗清理、过期响应守卫三处都靠它
 * 认领自己那一次请求 —— 所以它必须**总是能生成一个合规的 v4 uuid**，
 * 不能在某类运行环境里直接抛错。
 *
 * 为什么要兜底：`crypto.randomUUID()` 只在 secure context 可用（MDN 明确），
 * 打包后的 macOS 应用走 `tauri://localhost` 自定义协议，是否算 secure context
 * 没有保证。真抛错的话，事件处理器会同步中断 —— 用户看到的是"点了没反应"，
 * 而 jsdom 里 `crypto.randomUUID` 存在，单测根本抓不到这条路。
 * `crypto.getRandomValues` 没有这个限制，退化实现拼出的串形状与 v4 完全一致。
 */

/** v4 uuid 的形状：版本位固定 4、变体位固定 8/9/a/b。 */
const UUID_V4_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

/** 用 getRandomValues 拼一个标准 v4 uuid（版本位与变体位按规范改写）。 */
function randomUuidFromRandomValues(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  // 版本位：第 7 字节高四位 = 0100
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  // 变体位：第 9 字节高两位 = 10
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * 生成一次请求的号。优先用平台自带的 `randomUUID`（快且由平台保证质量），
 * 不可用时退到 `getRandomValues` —— 两条路径产出的都是合规 v4。
 */
export function createRequestId(): string {
  if (typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return randomUuidFromRandomValues();
}

/** 供测试与后端契约共用：形状必须是标准 v4。 */
export { UUID_V4_PATTERN };
