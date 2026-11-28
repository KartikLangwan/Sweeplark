import { api } from "./api";
import type { AppInfo, Settings } from "./types";

export type Page = "home" | "boost" | "optimizer" | "clean" | "monitor" | "storage" | "apps" | "startup" | "health" | "tools" | "assistant" | "inbox" | "history" | "settings";

export const ui = $state({
  page: "home" as Page,
  info: null as AppInfo | null,
  settings: null as Settings | null,
  unread: 0,
  pendingApprovals: 0,
  theme: "light" as "light" | "dark",
  toasts: [] as { id: number; text: string; tone: "good" | "warn" | "info" }[],
});

let toastId = 0;
export function toast(text: string, tone: "good" | "warn" | "info" = "good") {
  const id = ++toastId;
  ui.toasts.push({ id, text, tone });
  setTimeout(() => (ui.toasts = ui.toasts.filter((t) => t.id !== id)), 4200);
}

/** Pick the UI text for the current language (English by default). */
export function t(en: string, hi: string): string {
  return ui.settings?.language === "hinglish" ? hi : en;
}

export function go(page: Page) {
  ui.page = page;
}

let inboxLoaded = false;
export async function refreshInbox() {
  const box = await api.inbox();
  // Chime only for a request that arrives while the app is open, not for old ones at start.
  const newApproval = inboxLoaded && box.pending_approvals > ui.pendingApprovals;
  inboxLoaded = true;
  ui.unread = box.unread;
  ui.pendingApprovals = box.pending_approvals;
  if (newApproval) chime();
  return box;
}

/** Save settings. If they were changed elsewhere meanwhile (tray, AI download), the fresh
 *  copy is loaded instead and the caller gets an error to show. */
export async function saveSettings(s: Settings): Promise<Settings> {
  try {
    const saved = await api.saveSettings(s);
    ui.settings = saved;
    return saved;
  } catch (e) {
    if (String(e).includes("settings_changed")) {
      ui.settings = await api.settings();
      throw new Error(t("Settings were changed elsewhere and have been reloaded. Please try again.", "Settings kahin aur badli gayi thi, dobara load ki. Phir se try karein."));
    }
    throw e;
  }
}

/** Soft two-note chime (generated, no audio file) for approval requests. */
export function chime() {
  const n = ui.settings?.agent.notify;
  if (!n || !n.enabled || !n.sound) return;
  try {
    const ctx = new AudioContext();
    const notes = [880, 1318.5];
    notes.forEach((f, i) => {
      const o = ctx.createOscillator(), g = ctx.createGain();
      o.type = "sine"; o.frequency.value = f;
      const t = ctx.currentTime + i * 0.14;
      g.gain.setValueAtTime(0, t); g.gain.linearRampToValueAtTime(0.18, t + 0.02); g.gain.exponentialRampToValueAtTime(0.0001, t + 0.5);
      o.connect(g).connect(ctx.destination); o.start(t); o.stop(t + 0.55);
    });
  } catch { /* audio not available */ }
}

/** Two themes: soft light and dark. The first start follows Windows; after that, the user's pick. */
export function initialTheme(): "light" | "dark" {
  try {
    const saved = localStorage.getItem("theme");
    if (saved === "light" || saved === "dark") return saved;
  } catch { /* ignore */ }
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(t: "light" | "dark", remember = true) {
  ui.theme = t;
  document.documentElement.setAttribute("data-theme", t);
  if (remember) {
    try { localStorage.setItem("theme", t); } catch { /* ignore */ }
  }
}
