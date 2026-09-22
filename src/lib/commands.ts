import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AgentAction,
  AgentConfigInfo,
  AgentResult,
  AgentRun,
  ClipGroup,
  ClipboardItem,
  ListQuery,
  ListResult,
  UpdateStatus,
} from "@/types/clipboard";

export const commands = {
  listClipboardItems(query: ListQuery): Promise<ListResult> {
    return invoke("list_clipboard_items", { query });
  },
  deleteClipboardItem(id: string): Promise<void> {
    return invoke("delete_clipboard_item", { id });
  },
  setItemGroup(id: string, groupId: string | null): Promise<void> {
    return invoke("set_item_group", { id, groupId });
  },
  copyClipboardItem(id: string): Promise<void> {
    return invoke("copy_clipboard_item", { id });
  },
  pasteClipboardItem(id: string): Promise<void> {
    return invoke("paste_clipboard_item", { id });
  },
  /** 把一段文本写进剪贴板（多选合并复制用），不写入历史记录。 */
  copyTextToClipboard(text: string): Promise<void> {
    return invoke("copy_text_to_clipboard", { text });
  },
  /** 合并复制后直接粘贴：写剪贴板 → 隐藏窗口 → 模拟 ⌘V。 */
  pasteText(text: string): Promise<void> {
    return invoke("paste_text", { text });
  },
  /**
   * 解析来源应用的真实图标，返回 {应用名: PNG 绝对路径}。
   * 系统里找不到的应用不会出现在结果里（前端退回 emoji）。
   */
  getSourceIcons(apps: string[]): Promise<Record<string, string>> {
    return invoke("get_source_icons", { apps });
  },
  clearHistory(keepGrouped: boolean): Promise<number> {
    return invoke("clear_history", { keepGrouped });
  },

  // Groups
  listGroups(): Promise<ClipGroup[]> {
    return invoke("list_groups");
  },
  createGroup(dto: { name: string; color: string }): Promise<ClipGroup> {
    return invoke("create_group", { dto });
  },
  updateGroup(dto: { id: string; name: string; color: string }): Promise<ClipGroup> {
    return invoke("update_group", { dto });
  },
  deleteGroup(id: string): Promise<void> {
    return invoke("delete_group", { id });
  },

  // Settings
  getSettings(): Promise<AppSettings> {
    return invoke("get_settings");
  },
  updateSettings(settings: AppSettings): Promise<AppSettings> {
    return invoke("update_settings", { settings });
  },
  setCaptureEnabled(enabled: boolean): Promise<AppSettings> {
    return invoke("set_capture_enabled", { enabled });
  },
  updateShortcut(shortcut: string): Promise<AppSettings> {
    return invoke("update_shortcut", { shortcut });
  },

  // 系统集成
  /** 读实际生效的开机自启状态（查系统，不查数据库）。 */
  getAutostartEnabled(): Promise<boolean> {
    return invoke("get_autostart_enabled");
  },
  setAutostartEnabled(enabled: boolean): Promise<void> {
    return invoke("set_autostart_enabled", { enabled });
  },

  // 更新
  /** 检查应用更新。实时查询 GitHub 最新 release 并比较版本。 */
  checkForUpdates(): Promise<UpdateStatus> {
    return invoke("check_for_updates");
  },
  /** 应用内一键更新：下载、验签、安装后自动重启。进度见 update-progress 事件。 */
  downloadAndInstallUpdate(): Promise<void> {
    return invoke("download_and_install_update");
  },

  // Agent：后端按 item ID 读取内容并执行本地安全检查，前端不直接请求模型。
  runAgentAction(itemId: string, action: AgentAction): Promise<AgentResult> {
    return invoke("run_agent_action", { request: { item_id: itemId, action } });
  },
  /** 多条内容一次归纳。内容同样只能由已保存的条目 ID 提供。 */
  runAgentActionBatch(itemIds: string[], action: AgentAction): Promise<AgentResult> {
    return invoke("run_agent_action_batch", { request: { item_ids: itemIds, action } });
  },
  /** 取消进行中的 AI 请求；返回是否确有请求被取消。 */
  cancelAgentAction(): Promise<boolean> {
    return invoke("cancel_agent_action");
  },
  /** 读取模型配置快照（掩码，不含密钥本体）。 */
  getAgentConfig(): Promise<AgentConfigInfo> {
    return invoke("get_agent_config");
  },
  /** 保存 API Key 到系统钥匙串。返回值是更新后的快照。 */
  saveAgentKey(apiKey: string): Promise<AgentConfigInfo> {
    return invoke("save_agent_key", { request: { api_key: apiKey } });
  },
  clearAgentKey(): Promise<AgentConfigInfo> {
    return invoke("clear_agent_key");
  },
  /** 保存自定义服务地址与模型名（任何 OpenAI 兼容端点）。 */
  saveAgentEndpoint(baseUrl: string, model: string): Promise<AgentConfigInfo> {
    return invoke("save_agent_endpoint", { request: { base_url: baseUrl, model } });
  },
  /** 恢复内置默认地址与模型。 */
  resetAgentEndpoint(): Promise<AgentConfigInfo> {
    return invoke("reset_agent_endpoint");
  },
  /** 用 GET /models 验证 Key 与网络，不产生推理费用。 */
  testAgentConnection(): Promise<void> {
    return invoke("test_agent_connection");
  },
  /** 最近的 AI 使用记录，新的在前。 */
  listAgentRuns(): Promise<AgentRun[]> {
    return invoke("list_agent_runs");
  },
  /** 取一条使用记录；记录已随原始条目被删时返回 null。 */
  getAgentRun(id: string): Promise<AgentRun | null> {
    return invoke("get_agent_run", { id });
  },
  /** 清空使用记录，返回删掉的条数。 */
  clearAgentRuns(): Promise<number> {
    return invoke("clear_agent_runs");
  },
};

export type { ClipboardItem };
