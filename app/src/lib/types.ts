// Shapes returned by the Rust side (serde). Keep in sync with sweeplark-core.

export type Risk = "safe" | "ask" | "expert";
export type Severity = "good" | "info" | "warning" | "critical";
export type Tier = "lite" | "balanced" | "power";

export interface SampleFile { path: string; bytes: number }
export interface FlaggedFile { path: string; bytes: number; kind: string; review: string | null }
export type PreviewStatus =
  | { kind: "ready" } | { kind: "empty" } | { kind: "needs_admin" } | { kind: "app_running"; apps: string[] };

export interface CleanerPreview {
  id: string; name: string; description: string; category: string; risk: Risk;
  needs_admin: boolean; privacy: boolean; note: string | null; status: PreviewStatus;
  files: number; bytes: number; sample: SampleFile[]; blocked: string[];
  flagged: FlaggedFile[]; flagged_bytes: number;
}
export interface ScanReport { cleaners: CleanerPreview[]; total_files: number; total_bytes: number; duration_ms: number }
export interface CleanOutcome {
  id: string; name: string; deleted_files: number; freed_bytes: number; in_use: number; changed: number;
  kept_for_approval: number; recycled: number; skipped_reason: string | null; errors: string[];
}
export interface CleanReport { outcomes: CleanOutcome[]; freed_bytes: number; deleted_files: number; ledger_id: number | null }

export interface StartupItem {
  id: string; name: string; command: string; source: string; source_label: string; enabled: boolean;
  needs_admin: boolean; advice: "keep" | "disable" | "neutral"; advice_note: string | null;
}

export interface AppUsage { name: string; processes: number; memory_bytes: number; cpu_percent: number; system: boolean }
export interface DiskInfo {
  mount: string; name: string; file_system: string; kind: string; total_bytes: number; free_bytes: number;
  removable: boolean; is_system: boolean;
}
export interface Health {
  taken_at: string; os: string; host_name: string; uptime_secs: number; is_admin: boolean; cpu_brand: string;
  cpu_cores: number; cpu_percent: number; memory_total: number; memory_used: number; swap_total: number;
  swap_used: number; process_count: number; disks: DiskInfo[];
  gpus: { name: string; memory_bytes: number | null; dedicated: boolean }[];
  top_memory: AppUsage[]; top_cpu: AppUsage[]; temperatures: { label: string; celsius: number }[];
  battery: { percent: number | null; charging: boolean | null; on_ac_power: boolean | null;
    design_capacity_mwh: number | null; full_charge_capacity_mwh: number | null } | null;
  security: { defender_antivirus_enabled: boolean | null; defender_realtime_enabled: boolean | null;
    defender_signature_age_days: number | null; firewall_profiles: [string, boolean][] } | null;
  physical_disks: { name: string; media: string; health: string; size_bytes: number }[];
}

export type SuggestedAction =
  | { kind: "clean" } | { kind: "review_startup" } | { kind: "restart" } | { kind: "open_windows_security" }
  | { kind: "backup" } | { kind: "close_apps"; apps: string[] } | { kind: "boost" } | { kind: "optimize" } | { kind: "apps" };
export interface Finding {
  id: string; severity: Severity; topics: string[]; title: string; detail: string; action: SuggestedAction | null;
}

export interface OptimizePlan {
  health: Health; scan: ScanReport; startup: StartupItem[]; findings: Finding[];
  auto_fix_ids: string[]; auto_fix_bytes: number; startup_suggestions: string[];
}
export interface OptimizeReport {
  clean: CleanReport; system_free_before: number | null; system_free_after: number | null; left_for_you: Finding[];
}

export interface ChangeRecord {
  id: number; at: string; category: string; title: string; risk: Risk; status: string; bytes_freed: number;
  items: number; can_undo: boolean;
}
export interface History { summary: { changes: number; bytes_freed: number; undoable: number }; rows: ChangeRecord[] }

export type MessageKind = "alert" | "daily" | "weekly" | "cleanup" | "approval" | "tip";
export interface Message {
  id: number; at: string; kind: MessageKind; title: string; body: string; ai_body: string | null; read: boolean;
  data: Record<string, unknown>;
}
export interface Inbox { messages: Message[]; unread: number; pending_approvals: number }
export interface ApprovalDetail {
  cleaners: { id: string; name: string; bytes: number; files: number; risk: Risk; note: string | null }[];
  risky_files: FlaggedFile[]; total_bytes: number; ai_reviewed: number;
}
export interface Approval { id: number; at: string; status: string; title: string; detail: ApprovalDetail }

export interface ModelInfo {
  tier: Tier; name: string; repo: string; file: string; size: number; license: string; context: number;
  style: "brief" | "full"; description: string; installed: boolean; fits: boolean;
}
export interface InstallProgress { stage: string; tier: Tier | null; done: number; total: number; error: string | null }
export interface AiStatus {
  enabled: boolean; tier: Tier | null; recommended: Tier; models: ModelInfo[];
  install: { runtime_installed: boolean; runtime_gpu: boolean; installed_tiers: Tier[]; disk_bytes: number };
  loaded: Tier | null; progress: InstallProgress | null; ram_total: number; has_gpu: boolean;
}
export interface AssistantReply {
  text: string; source: "model" | "rules"; key_points: Finding[]; tools: string[]; actions: SuggestedAction[];
  model: string | null;
}
export type ChatEvent = { event: "status"; data: string } | { event: "token"; data: string };
export interface ChatTurn { role: "user" | "assistant"; content: string }

export interface Settings {
  trust_level: "safe" | "ask" | "expert";
  whitelist: string[];
  recycle_bin_min_age_days: number;
  privacy_consent: boolean;
  disabled_cleaners: string[];
  enabled_cleaners: string[];
  language: "en" | "hinglish";
  agent: {
    enabled: boolean; start_with_windows: boolean; sample_minutes: number;
    daily_summary: string | null; weekly_summary: { weekday: number; time: string } | null;
    smart_alerts: boolean;
    auto_clean: { frequency: "off" | "daily" | "weekly"; time: string; weekday: number; only_when_idle: boolean; when_disk_low: boolean };
    approval: "ask_always" | "risky_only";
    notify: { enabled: boolean; sound: boolean; max_per_day: number; quiet_from: string; quiet_to: string };
  };
  ai: {
    enabled: boolean; tier: Tier | null; use_gpu: boolean; unload_after_secs: number;
    review_files: boolean; write_summaries: boolean;
  };
  check_updates: boolean;
  skipped_version: string | null;
  boost: { never: string[]; auto_game: boolean; ram_guard: boolean; ram_guard_percent: number };
  restore_point_before_tweaks: boolean;
}
export interface AppInfo {
  name: string; tagline: string; tagline_hi: string; version: string; is_admin: boolean; os: string;
  developer: string; developer_title: string; developer_tagline: string; developer_url: string; repo: string; portable: boolean;
}
export interface UpdateInfo {
  version: string; notes: string; published_at: string; page_url: string; setup_url: string | null; setup_size: number; setup_sha256: string | null;
}

export type TweakCategory = "performance" | "gaming" | "gpu" | "power" | "privacy" | "ai" | "services" | "explorer" | "taskbar" | "system";
export interface TweakState {
  id: string; name: string; description: string; category: TweakCategory; risk: Risk; recommended: boolean; setting: boolean;
  restart: "explorer" | "signout" | "reboot" | null; note: string | null; source: string | null; needs_admin: boolean;
  available: boolean; applied: boolean; can_undo: boolean;
}
export interface LiveStats {
  cpu_percent: number; per_core: number[]; cpu_mhz: number; cpu_temp: number | null; board_temp: number | null; ram_used: number; ram_total: number;
  swap_used: number; swap_total: number; gpu_name: string | null; gpu_percent: number | null; gpu_temp: number | null;
  vram_used: number | null; vram_total: number | null; net_down_bps: number; net_up_bps: number;
  disk_read_bps: number | null; disk_write_bps: number | null; processes: number;
}
export interface ProcessGroup { name: string; pids: number[]; cpu_percent: number; memory_bytes: number; system: boolean }
export interface SizedPath { path: string; bytes: number; files: number; modified?: number }
export interface DuplicateGroup { bytes: number; files: SizedPath[] }
export interface DuplicateReport { root: string; groups: DuplicateGroup[]; wasted_bytes: number; scanned_files: number; complete: boolean; duration_ms: number }
export interface InstalledApp { id: string; name: string; publisher: string | null; version: string | null; size_bytes: number | null; install_date: string | null; location: string | null; for_all_users: boolean }
export interface AppUpdate { name: string; id: string; current: string; available: string }
export interface JobProgress { job: string; done: number; total: number; text: string }
export interface StorageReport {
  root: string; total_bytes: number; total_files: number; folders: SizedPath[]; big_files: SizedPath[];
  by_kind: [string, number][]; complete: boolean; duration_ms: number;
}
export type ToolId = "flush_dns" | "restart_explorer" | "rebuild_icon_cache" | "sfc" | "dism" | "reset_network" | "reset_windows_update" | "disk_check" | "cancel_shutdown" | "component_cleanup" | "restore_point";
export interface PowerPlan { guid: string; name: string; active: boolean }
export interface BoostApp { name: string; pids: number[]; memory_bytes: number; cpu_percent: number; gpu_percent: number }
export interface BoostPlan { ram_used: number; ram_total: number; cpu_percent: number; gpu_percent: number | null; foreground: string | null; apps: BoostApp[]; protected: string[]; can_purge_standby: boolean; on_battery: boolean }
export interface BoostOptions { trim_ram: boolean; purge_standby: boolean; eco_cpu: boolean; eco_gpu: boolean; high_performance: boolean; apps: string[] | null }
export interface BoostReport { ram_before: number; ram_after: number; trimmed: number; standby_purged: boolean; eco: string[]; power_plan: string | null; previous_plan: string | null; errors: string[]; ledger_id: number | null }
