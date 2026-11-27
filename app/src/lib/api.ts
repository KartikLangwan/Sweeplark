// Bridge to the Rust side. Outside the desktop app (plain browser preview) a small mock
// with realistic data is used, so the UI can be designed and tested anywhere.

import type {
  AiStatus, AppInfo, Approval, AssistantReply, ChatEvent, ChatTurn, CleanReport, ChangeRecord, Health, History,
  Inbox, OptimizePlan, UpdateInfo, TweakState, LiveStats, ProcessGroup, StorageReport, ToolId, PowerPlan, BoostPlan, BoostOptions, BoostReport, OptimizeReport, ScanReport, Settings, StartupItem, Tier, InstallProgress,
} from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  const { mock } = await import("./mock");
  return mock<T>(cmd, args);
}

export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => handler(e.payload));
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  health: (slow = false) => call<Health>("get_health", { slow }),
  scan: (expert = false, privacy = false) => call<ScanReport>("scan", { expert, privacy }),
  clean: (ids: string[], confirmed: boolean, approvedFiles: string[] = []) =>
    call<CleanReport>("clean", { ids, confirmed, approvedFiles }),
  startupList: () => call<StartupItem[]>("startup_list"),
  startupSet: (id: string, enabled: boolean) => call<number>("startup_set", { id, enabled }),
  history: (limit = 100) => call<History>("history", { limit }),
  undo: (id: number) => call<ChangeRecord>("undo", { id }),
  optimizePlan: () => call<OptimizePlan>("optimize_plan"),
  optimizeApply: () => call<OptimizeReport>("optimize_apply"),
  settings: () => call<Settings>("settings_get"),
  saveSettings: (settings: Settings) => call<void>("settings_set", { settings }),
  inbox: () => call<Inbox>("inbox_list"),
  markRead: (id: number | null) => call<void>("inbox_read", { id }),
  approvals: () => call<Approval[]>("approvals_list"),
  decide: (id: number, approve: boolean, files: string[]) =>
    call<CleanReport | null>("approval_decide", { id, approve, files }),
  aiStatus: () => call<AiStatus>("ai_status"),
  aiInstall: (tier: Tier, cancel = false) => call<void>("ai_install", { tier, cancel }),
  aiRemove: (tier: Tier) => call<void>("ai_remove", { tier }),
  aiUnload: () => call<void>("ai_unload"),
  efficiency: (name: string, on: boolean) => call<number>("efficiency_mode", { name, on }),
  openSecurity: () => call<void>("open_windows_security"),
  relaunchAdmin: () => call<void>("relaunch_as_admin"),
  updateCheck: (manual = false) => call<UpdateInfo | null>("update_check", { manual }),
  updateInstall: () => call<void>("update_install"),
  updateSkip: (version: string) => call<void>("update_skip", { version }),
  tweaks: () => call<TweakState[]>("tweaks_list"),
  tweakSet: (id: string, on: boolean) => call<void>("tweak_set", { id, on }),
  tweaksApply: (ids: string[]) => call<{ applied: string[]; skipped: [string, string][] }>("tweaks_apply_recommended", { ids }),
  live: () => call<LiveStats>("live_stats"),
  processes: () => call<ProcessGroup[]>("process_list"),
  endProcess: (name: string) => call<number>("process_end", { name }),
  storage: (path?: string) => call<StorageReport>("storage_analyze", { path: path ?? null }),
  recycle: (path: string) => call<void>("storage_recycle", { path }),
  reveal: (path: string) => call<void>("reveal_path", { path }),
  tool: (tool: ToolId) => call<string>("tool_run", { tool }),
  shutdown: (minutes: number, restart: boolean) => call<void>("shutdown_schedule", { minutes, restart }),
  windowsTool: (name: string) => call<void>("windows_tool", { name }),
  hardware: () => call<{ items: [string, string][] }>("hardware_info"),
  powerPlans: () => call<PowerPlan[]>("power_plans"),
  setPowerPlan: (guid: string | null) => call<void>("power_plan_set", { guid }),
  boostPlan: () => call<BoostPlan>("boost_plan"),
  boostApply: (options: BoostOptions) => call<BoostReport>("boost_apply", { options }),
  openUrl: (url: string) => (inTauri ? call<void>("open_url", { url }) : Promise.resolve(void window.open(url, "_blank"))),

  async chat(question: string, history: ChatTurn[], onEvent: (e: ChatEvent) => void): Promise<AssistantReply> {
    if (inTauri) {
      const { invoke, Channel } = await import("@tauri-apps/api/core");
      const channel = new Channel<ChatEvent>();
      channel.onmessage = onEvent;
      return invoke<AssistantReply>("ai_chat", { question, history, onEvent: channel });
    }
    const { mockChat } = await import("./mock");
    return mockChat(question, onEvent);
  },
};

export type { InstallProgress };
