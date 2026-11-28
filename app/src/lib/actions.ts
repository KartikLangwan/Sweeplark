// What the suggestion buttons under findings and assistant answers say and do.
// They only open pages or Windows tools: the user still decides there.

import { api } from "./api";
import { go, t, toast } from "./state.svelte";
import type { SuggestedAction } from "./types";

export function actionLabel(a: SuggestedAction): string {
  switch (a.kind) {
    case "clean": return t("Review cleanup", "Safai dekhein");
    case "review_startup": return t("Review startup", "Startup dekhein");
    case "restart": return t("How to restart", "Restart ki yaad");
    case "open_windows_security": return t("Open Windows Security", "Windows Security kholein");
    case "close_apps": return t("See heavy apps", "Bhari apps dekhein");
    case "backup": return t("How to back up", "Backup kaise lein");
    case "boost": return t("Open Boost", "Boost kholein");
    case "optimize": return t("Open Optimizer", "Optimizer kholein");
    case "apps": return t("Open Apps", "Apps kholein");
  }
}

export function actionIcon(a: SuggestedAction): string {
  switch (a.kind) {
    case "clean": return "broom";
    case "review_startup": return "leaf";
    case "boost": return "rocket";
    case "optimize": return "sparkle";
    case "apps": return "apps";
    case "open_windows_security": return "shield";
    case "close_apps": return "cpu";
    default: return "info";
  }
}

export function runAction(a: SuggestedAction): void {
  switch (a.kind) {
    case "clean": return go("clean");
    case "review_startup": return go("startup");
    case "close_apps": return go("boost");
    case "boost": return go("boost");
    case "optimize": return go("optimizer");
    case "apps": return go("apps");
    case "open_windows_security": void api.openSecurity(); return;
    case "restart": return toast(t("Save your work, then Start → Power → Restart.", "Kaam save karke Start → Power → Restart karein."), "info");
    case "backup": return toast(t("Copy important files to a USB drive or cloud storage.", "Zaruri files ko pen drive ya cloud par copy karein."), "info");
  }
}
