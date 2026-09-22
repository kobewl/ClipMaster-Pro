import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AgentAction,
  AgentConfigInfo,
  AgentResult,
  AgentRun,
  AgentSessionDetail,
  AgentSessionSummary,
  ClipGroup,
  ClearDerivedDataResult,
  ClipboardItem,
  ListQuery,
  ListResult,
  PlannerSuggestionSet,
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
  /**
   * requestId 由前端生成（crypto.randomUUID）而不是后端：invoke 要等请求结束
   * 才返回，后端在请求开始时才生成的号前端在取消时拿不到。前端在发起那一刻
   * 就握着它 —— 取消、关窗清理、过期响应守卫用的都是同一个号。
   */
  runAgentAction(itemId: string, action: AgentAction, requestId: string): Promise<AgentResult> {
    return invoke("run_agent_action", {
      request: { item_id: itemId, action, request_id: requestId },
    });
  },
  /** 多条内容一次归纳。内容同样只能由已保存的条目 ID 提供。 */
  runAgentActionBatch(
    itemIds: string[],
    action: AgentAction,
    requestId: string,
  ): Promise<AgentResult> {
    return invoke("run_agent_action_batch", {
      request: { item_ids: itemIds, action, request_id: requestId },
    });
  },
  /**
   * 按号取消进行中的 AI 请求；返回是否确有请求被取消。
   * 号不匹配时后端返回 false 且不打断任何请求 —— 不会取消错对象。
   */
  cancelAgentAction(requestId: string): Promise<boolean> {
    return invoke("cancel_agent_action", { requestId });
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

  // Flow 会话（Phase 1 第 3 步）：纯本地派生数据，不调模型、不发网络请求。
  /**
   * 会话列表，新的在前。
   *
   * **带重算副作用**（关键判断 10）：打开工作台的那一次调用会在服务端先按
   * 「最近 3 天 ∩ 最近 200 条」重算一遍再返回。这是「会话只在用户打开工作台时
   * 计算」的落地点 —— 不是刷新缓存，而是这次调用本身会写派生表（幂等）。
   */
  listAgentSessions(): Promise<AgentSessionSummary[]> {
    return invoke("list_agent_sessions");
  },
  /** 会话详情；会话已随它最后一条成员被删掉时返回 `null`（不是错误）。 */
  getAgentSession(id: string): Promise<AgentSessionDetail | null> {
    return invoke("get_agent_session", { id });
  },
  /** 用户显式保存一次会话（多选 ≥2 条 →「存为会话」）。不重算，不覆盖他的选择。 */
  createAgentSession(itemIds: string[]): Promise<AgentSessionDetail> {
    return invoke("create_agent_session", { request: { item_ids: itemIds } });
  },
  /** 删一条会话，返回是否真的删掉了（`false` = 已经不在了，同样不是错误）。 */
  deleteAgentSession(id: string): Promise<boolean> {
    return invoke("delete_agent_session", { id });
  },
  /** 一键清除所有 AI 派生数据（会话 + 使用记录）；原始剪贴板记录不受影响。 */
  clearAgentDerivedData(): Promise<ClearDerivedDataResult> {
    return invoke("clear_agent_derived_data");
  },

  // Planner（Phase 1 第 5 步）：只在用户显式点「下一步建议」时调用 ——
  // 打开工作台不等于授权花钱（会触发一次真实模型调用与计费）。
  /**
   * 在一条会话上求「下一步建议」。
   *
   * **建议不落库**：返回值就是建议本体，关掉工作台即弃（存建议等于存模型响应正文，
   * 与审计的隐私边界冲突）。requestId 由前端预置，理由同 `runAgentAction`：
   * 界面要能按号取消、迟到响应要做守卫，号必须在发起那一刻就握在手里。
   */
  suggestSessionActions(sessionId: string, requestId: string): Promise<PlannerSuggestionSet> {
    return invoke("suggest_session_actions", {
      request: { session_id: sessionId, request_id: requestId },
    });
  },
};

export type { ClipboardItem };
