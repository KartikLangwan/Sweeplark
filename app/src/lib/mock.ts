// Browser-preview data only. Never used inside the desktop app.
import type {
  AiStatus, AppInfo, Approval, AssistantReply, ChatEvent, CleanReport, Finding, Health, History, Inbox,
  OptimizePlan, OptimizeReport, ScanReport, Settings, StartupItem,
} from "./types";

const GB = 1024 ** 3, MB = 1024 ** 2;
const now = Date.now();
const iso = (minsAgo: number) => new Date(now - minsAgo * 60000).toISOString();

const health: Health = {
  taken_at: iso(0), os: "Windows 11 Home 24H2", host_name: "AMIT-LAPTOP", uptime_secs: 9 * 86400 + 4 * 3600,
  is_admin: false, cpu_brand: "Intel Core i5-1135G7", cpu_cores: 8, cpu_percent: 23, memory_total: 8 * GB,
  memory_used: 6.9 * GB, swap_total: 4 * GB, swap_used: 1.2 * GB, process_count: 214,
  disks: [
    { mount: "C:\\", name: "Windows", file_system: "NTFS", kind: "SSD", total_bytes: 237 * GB, free_bytes: 21.4 * GB, removable: false, is_system: true },
    { mount: "D:\\", name: "Data", file_system: "NTFS", kind: "HDD", total_bytes: 931 * GB, free_bytes: 402 * GB, removable: false, is_system: false },
  ],
  gpus: [{ name: "Intel Iris Xe Graphics", memory_bytes: null, dedicated: false }],
  top_memory: [
    { name: "chrome", processes: 24, memory_bytes: 2.4 * GB, cpu_percent: 6.1, system: false },
    { name: "Teams", processes: 7, memory_bytes: 1.1 * GB, cpu_percent: 2.3, system: false },
    { name: "MsMpEng", processes: 1, memory_bytes: 310 * MB, cpu_percent: 0.4, system: true },
    { name: "Code", processes: 11, memory_bytes: 820 * MB, cpu_percent: 1.2, system: false },
    { name: "explorer", processes: 1, memory_bytes: 190 * MB, cpu_percent: 0.3, system: true },
    { name: "Spotify", processes: 5, memory_bytes: 340 * MB, cpu_percent: 0.8, system: false },
  ],
  top_cpu: [
    { name: "chrome", processes: 24, memory_bytes: 2.4 * GB, cpu_percent: 6.1, system: false },
    { name: "Teams", processes: 7, memory_bytes: 1.1 * GB, cpu_percent: 2.3, system: false },
  ],
  temperatures: [{ label: "CPU Package", celsius: 61 }],
  battery: { percent: 76, charging: false, on_ac_power: false, design_capacity_mwh: 56000, full_charge_capacity_mwh: 43100 },
  security: { defender_antivirus_enabled: true, defender_realtime_enabled: true, defender_signature_age_days: 1,
    firewall_profiles: [["Domain", true], ["Private", true], ["Public", true]] },
  physical_disks: [{ name: "NVMe WDC SN530 256GB", media: "SSD", health: "Healthy", size_bytes: 256 * GB }],
};

const findings: Finding[] = [
  { id: "ram.high", severity: "warning", topics: ["performance"], title: "RAM 86% bhari hai",
    detail: "Sabse zyada RAM: chrome (2.4 GB), Teams (1.1 GB), Code (820 MB). Jo kaam mein nahi hai use band kar dein.",
    action: { kind: "close_apps", apps: ["chrome", "Teams", "Code"] } },
  { id: "junk.found", severity: "info", topics: ["storage"], title: "3.2 GB bekaar files mili",
    detail: "Temp files, cache aur purane logs. Personal files ko kuch nahi hoga.", action: { kind: "clean" } },
  { id: "startup.many", severity: "warning", topics: ["performance"], title: "11 startup apps chalu hain",
    detail: "Inhe band karna safe hai: Spotify, Discord, Microsoft Teams.", action: { kind: "review_startup" } },
  { id: "uptime.long", severity: "info", topics: ["performance"], title: "PC 9 din se restart nahi hua",
    detail: "Restart karne se atki memory saaf hoti hai aur updates install hote hain.", action: { kind: "restart" } },
];

const cleaner = (id: string, name: string, cat: string, risk: "safe" | "ask", bytes: number, files: number, extra: Partial<ScanReport["cleaners"][0]> = {}) => ({
  id, name, description: "", category: cat, risk, needs_admin: false, privacy: false, note: null,
  status: { kind: "ready" as const }, files, bytes,
  sample: [{ path: `C:\\Users\\amit\\AppData\\Local\\${name.replace(/ /g, "")}\\data_${files}`, bytes: Math.round(bytes / 6) }],
  blocked: [], flagged: [], flagged_bytes: 0, ...extra,
});

const scan: ScanReport = {
  duration_ms: 1840, total_files: 18342, total_bytes: 3.2 * GB,
  cleaners: [
    cleaner("windows.temp.user", "Temp files (aapke)", "system", "safe", 1.21 * GB, 5230, {
      description: "Apps ki chhodi hui temporary files.",
      flagged: [
        { path: "C:\\Users\\amit\\AppData\\Local\\Temp\\Rar$EXa12\\Invoice_March.pdf", bytes: 1.2 * MB, kind: "document", review: null },
        { path: "C:\\Users\\amit\\AppData\\Local\\Temp\\WhatsApp\\IMG_2041.jpg", bytes: 3.1 * MB, kind: "photo", review: "Photo user ki apni ho sakti hai" },
      ], flagged_bytes: 4.3 * MB }),
    cleaner("chrome.cache", "Google Chrome cache", "browser", "safe", 940 * MB, 9120, { description: "Websites ki cached files. Login aur history safe rahenge." }),
    cleaner("windows.thumbnails", "Thumbnail cache", "system", "safe", 212 * MB, 6),
    cleaner("discord.cache", "Discord cache", "apps", "safe", 388 * MB, 2210, { status: { kind: "app_running", apps: ["discord.exe"] } }),
    cleaner("gpu.shader_cache.directx", "DirectX shader cache", "games", "ask", 296 * MB, 801, { note: "Pehli baar game kholne par thodi der atak aa sakti hai." }),
    cleaner("windows.recycle_bin", "Recycle Bin (purani cheezein)", "recycle_bin", "ask", 180 * MB, 41, { note: "Recycle Bin se hatayi files wapas nahi aatin." }),
    cleaner("windows.update.downloads", "Windows Update ka bacha data", "system", "ask", 0, 0, { needs_admin: true, status: { kind: "needs_admin" } }),
    cleaner("edge.cache", "Microsoft Edge cache", "browser", "safe", 0, 0, { status: { kind: "empty" } }),
  ],
};

const startup: StartupItem[] = [
  { id: "run-user:Spotify", name: "Spotify", command: "C:\\Users\\amit\\AppData\\Roaming\\Spotify\\Spotify.exe /background", source: "run_user", source_label: "Registry (sirf aap)", enabled: true, needs_admin: false, advice: "disable", advice_note: "Spotify zarurat par khud khol sakte hain." },
  { id: "run-user:Discord", name: "Discord", command: "Update.exe --processStart Discord.exe", source: "run_user", source_label: "Registry (sirf aap)", enabled: true, needs_admin: false, advice: "disable", advice_note: "Discord zarurat par khud khol sakte hain." },
  { id: "run-machine:SecurityHealth", name: "SecurityHealth", command: "%windir%\\system32\\SecurityHealthSystray.exe", source: "run_machine", source_label: "Registry (sab users)", enabled: true, needs_admin: true, advice: "keep", advice_note: "Windows Security ka icon aur alerts. Chalu rakhein." },
  { id: "run-user:MicrosoftEdgeAutoLaunch", name: "MicrosoftEdgeAutoLaunch", command: "msedge.exe --no-startup-window", source: "run_user", source_label: "Registry (sirf aap)", enabled: true, needs_admin: false, advice: "disable", advice_note: "Edge ko pehle se chalu rakhta hai. Band karne se PC jaldi start hoga." },
  { id: "run-machine:RtkAudUService", name: "RtkAudUService", command: "RtkAudUService64.exe -background", source: "run_machine", source_label: "Registry (sab users)", enabled: true, needs_admin: true, advice: "keep", advice_note: "Realtek audio driver." },
  { id: "folder-user:Notion.lnk", name: "Notion", command: "C:\\...\\Startup\\Notion.lnk", source: "folder_user", source_label: "Startup folder (sirf aap)", enabled: false, needs_admin: false, advice: "neutral", advice_note: null },
  { id: "run-user:OneDrive", name: "OneDrive", command: "OneDrive.exe /background", source: "run_user", source_label: "Registry (sirf aap)", enabled: true, needs_admin: false, advice: "disable", advice_note: "Band karne par files tab tak sync nahi hongi jab tak OneDrive khud na kholein." },
];

let settings: Settings = {
  trust_level: "ask", whitelist: [], recycle_bin_min_age_days: 7, privacy_consent: false, disabled_cleaners: [],
  enabled_cleaners: [], language: "en",
  agent: {
    enabled: true, start_with_windows: true, sample_minutes: 5, daily_summary: "21:00",
    weekly_summary: { weekday: 6, time: "20:00" }, smart_alerts: true,
    auto_clean: { frequency: "daily", time: "13:00", weekday: 6, only_when_idle: true, when_disk_low: true },
    approval: "ask_always",
    notify: { enabled: true, sound: true, max_per_day: 4, quiet_from: "22:30", quiet_to: "08:00" },
  },
  ai: { enabled: true, tier: "balanced", use_gpu: true, unload_after_secs: 120, review_files: true, write_summaries: true },
};

const approvals: Approval[] = [{
  id: 7, at: iso(18), status: "pending", title: "1.6 GB saaf karne ki ijazat chahiye (2 files aapki ho sakti hain)",
  detail: {
    total_bytes: 1.6 * GB, ai_reviewed: 6,
    cleaners: [
      { id: "windows.temp.user", name: "Temp files (aapke)", bytes: 1.2 * GB, files: 5228, risk: "safe", note: null },
      { id: "chrome.cache", name: "Google Chrome cache", bytes: 410 * MB, files: 3120, risk: "safe", note: null },
    ],
    risky_files: scan.cleaners[0].flagged,
  },
}];

const inbox: Inbox = {
  unread: 2, pending_approvals: 1,
  messages: [
    { id: 12, at: iso(18), kind: "approval", title: approvals[0].title, body: "2 files documents/photos jaisi lagti hain; unhe aap ek-ek karke chun sakte hain.", ai_body: null, read: false, data: { approval_id: 7 } },
    { id: 11, at: iso(60 * 15), kind: "daily", title: "Aaj ki report", body: "• Aaj 1.4 GB jagah khaali hui (1 safai)\n• RAM ausat 71%, sabse zyada 93% (chrome sabse bhari)\n• CPU ausat 18%, sabse zyada 64%\n• System drive mein 21.4 GB khaali hai", ai_body: "Aaj PC ne theek kaam kiya: 1.4 GB jagah khaali hui. Shaam ko RAM 93% tak gayi thi, zyada tar chrome ki wajah se; kam tabs rakhne se PC aur tez chalega.", read: false, data: {} },
    { id: 10, at: iso(60 * 26), kind: "alert", title: "Nayi startup app: Notion", body: "Notion ab Windows ke saath apne aap chalegi. Agar roz zarurat nahi, toh Startup mein band kar sakte hain.", ai_body: null, read: true, data: {} },
    { id: 9, at: iso(60 * 50), kind: "alert", title: "RAM 92% bhari hai", body: "15 minute se RAM lagbhag poori bhari hai. Sabse zyada chrome (2.8 GB) le raha hai.", ai_body: null, read: true, data: {} },
  ],
};

const history: History = {
  summary: { changes: 6, bytes_freed: 7.8 * GB, undoable: 2 },
  rows: [
    { id: 6, at: iso(60 * 15), category: "clean", title: "Cleaned 1.4 GB (6120 files)", risk: "safe", status: "applied", bytes_freed: 1.4 * GB, items: 6120, can_undo: false },
    { id: 5, at: iso(60 * 30), category: "startup", title: "Startup se band kiya: Notion", risk: "ask", status: "applied", bytes_freed: 0, items: 1, can_undo: true },
    { id: 4, at: iso(60 * 72), category: "optimize", title: "Efficiency mode on: Teams (7 process)", risk: "ask", status: "applied", bytes_freed: 0, items: 7, can_undo: false },
    { id: 3, at: iso(60 * 100), category: "startup", title: "Startup se band kiya: Skype", risk: "ask", status: "applied", bytes_freed: 0, items: 1, can_undo: true },
    { id: 2, at: iso(60 * 150), category: "clean", title: "Cleaned 6.4 GB (31002 files)", risk: "ask", status: "applied", bytes_freed: 6.4 * GB, items: 31002, can_undo: false },
  ],
};

const aiStatus: AiStatus = {
  enabled: true, tier: "balanced", recommended: "balanced", ram_total: 8 * GB, has_gpu: false, loaded: null, progress: null,
  install: { runtime_installed: true, runtime_gpu: false, installed_tiers: ["balanced"], disk_bytes: 1.32 * GB },
  models: [
    { tier: "lite", name: "Qwen3.5 0.8B", repo: "unsloth/Qwen3.5-0.8B-GGUF", file: "Qwen3.5-0.8B-Q4_K_M.gguf", size: 532517120, license: "Apache-2.0", context: 4096, style: "brief", description: "Sabse halka. 4-6 GB RAM wale purane PCs ke liye.", installed: false, fits: true },
    { tier: "balanced", name: "Qwen3.5 2B", repo: "unsloth/Qwen3.5-2B-GGUF", file: "Qwen3.5-2B-Q4_K_M.gguf", size: 1280835840, license: "Apache-2.0", context: 6144, style: "full", description: "Achha balance. 8 GB RAM wale zyadatar PCs ke liye.", installed: true, fits: true },
    { tier: "power", name: "Qwen3.5 4B", repo: "unsloth/Qwen3.5-4B-GGUF", file: "Qwen3.5-4B-Q4_K_M.gguf", size: 2740937888, license: "Apache-2.0", context: 8192, style: "full", description: "Sabse samajhdar. 16 GB RAM ya graphics card wale PCs ke liye.", installed: false, fits: false },
  ],
};

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function mock<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  await wait(cmd === "scan" || cmd === "optimize_plan" ? 900 : 120);
  const table: Record<string, unknown> = {
    app_info: { name: "Sweeplark", tagline: "Small bird. Clean PC.", tagline_hi: "Saaf PC, tez PC, aur control aapke haath mein.", version: "1.1.0", is_admin: false, os: "windows" } satisfies AppInfo,
    get_health: health,
    scan,
    clean: { freed_bytes: 2.36 * GB, deleted_files: 16541, ledger_id: 7,
      outcomes: [{ id: "windows.temp.user", name: "Temp files (aapke)", deleted_files: 5221, freed_bytes: 1.2 * GB, in_use: 7, changed: 0, kept_for_approval: 2, skipped_reason: null, errors: [] }] } satisfies CleanReport,
    startup_list: startup,
    startup_set: 8,
    history,
    undo: history.rows[1],
    optimize_plan: { health, scan, startup, findings, auto_fix_ids: ["windows.temp.user", "chrome.cache", "windows.thumbnails"], auto_fix_bytes: 2.36 * GB, startup_suggestions: ["run-user:Spotify", "run-user:Discord"] } satisfies OptimizePlan,
    optimize_apply: { clean: { freed_bytes: 2.36 * GB, deleted_files: 14352, ledger_id: 7, outcomes: [] }, system_free_before: 21.4 * GB, system_free_after: 23.8 * GB, left_for_you: findings.filter((f) => f.id !== "junk.found") } satisfies OptimizeReport,
    settings_get: settings,
    inbox_list: inbox,
    approvals_list: approvals,
    approval_decide: { freed_bytes: 1.6 * GB, deleted_files: 8348, ledger_id: 8, outcomes: [] },
    ai_status: aiStatus,
    efficiency_mode: 7,
  };
  if (cmd === "settings_set") settings = args.settings as Settings;
  if (cmd === "startup_set") {
    const it = startup.find((s) => s.id === args.id);
    if (it) it.enabled = args.enabled as boolean;
  }
  return (table[cmd] ?? null) as T;
}

export async function mockChat(question: string, onEvent: (e: ChatEvent) => void): Promise<AssistantReply> {
  onEvent({ event: "status", data: "PC ki jaankari dekh raha hoon…" });
  await wait(500);
  const text = question.toLowerCase().includes("kaun")
    ? "Main Sweeplark hoon, aapke PC ka offline assistant. Main PC ki sehat dekhta hoon aur safai ki salah deta hoon; aapki ijazat ke bina kuch nahi karta."
    : "Aapka PC sabse zyada RAM ki wajah se dheema hai: RAM 86% bhari hai aur chrome akela 2.4 GB le raha hai.\n- Jo tabs kaam mein nahi hain unhe band karein.\n- Startup mein Spotify aur Discord band karna safe hai.\n- 9 din se restart nahi hua; aaj ek baar restart karein.";
  for (const w of text.split(/(?<= )/)) {
    onEvent({ event: "token", data: w });
    await wait(18);
  }
  return { text, source: "model", key_points: findings.slice(0, 3), tools: ["specs", "performance", "problems"],
    actions: [{ kind: "close_apps", apps: ["chrome", "Teams"] }, { kind: "review_startup" }], model: "Qwen3.5 2B" };
}
