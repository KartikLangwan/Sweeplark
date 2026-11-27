import { api } from "./api";
import type { AppInfo, Settings } from "./types";

export type Page = "home" | "clean" | "startup" | "health" | "assistant" | "inbox" | "history" | "settings";

export const ui = $state({
  page: "home" as Page,
  info: null as AppInfo | null,
  settings: null as Settings | null,
  unread: 0,
  pendingApprovals: 0,
  theme: "system" as "system" | "light" | "dark",
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

export async function refreshInbox() {
  const box = await api.inbox();
  const newApproval = box.pending_approvals > ui.pendingApprovals;
  ui.unread = box.unread;
  ui.pendingApprovals = box.pending_approvals;
  if (newApproval) chime();
  return box;
}

export async function saveSettings(s: Settings) {
  await api.saveSettings(s);
  ui.settings = s;
}

/** Soft two-note chime (generated, no audio file) for approval requests. */
export function chime() {
  if (ui.settings && !ui.settings.agent.notify.sound) return;
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

export function applyTheme(t: "system" | "light" | "dark") {
  ui.theme = t;
  const root = document.documentElement;
  if (t === "system") root.removeAttribute("data-theme"); else root.setAttribute("data-theme", t);
  try { localStorage.setItem("theme", t); } catch { /* ignore */ }
}
