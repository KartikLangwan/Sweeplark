export function bytes(n: number | null | undefined): string {
  if (n == null) return "–";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n, u = 0;
  while (v >= 1024 && u < units.length - 1) { v /= 1024; u++; }
  if (u === 0) return `${n} B`;
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[u]}`;
}

export function pct(n: number): string {
  return `${Math.round(n)}%`;
}

import { t } from "./state.svelte";

export function ago(iso: string): string {
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 60) return t("just now", "abhi");
  if (s < 3600) return t(`${Math.floor(s / 60)} min ago`, `${Math.floor(s / 60)} min pehle`);
  if (s < 86400) return t(`${Math.floor(s / 3600)} h ago`, `${Math.floor(s / 3600)} ghante pehle`);
  const d = Math.floor(s / 86400);
  return d === 1 ? t("yesterday", "kal") : t(`${d} days ago`, `${d} din pehle`);
}

export function uptime(secs: number): string {
  const d = Math.floor(secs / 86400), h = Math.floor((secs % 86400) / 3600);
  return d > 0 ? t(`${d}d ${h}h`, `${d} din ${h} ghante`) : t(`${h}h`, `${h} ghante`);
}

export function shortPath(p: string, max = 64): string {
  if (p.length <= max) return p;
  return p.slice(0, 18) + "…" + p.slice(p.length - (max - 19));
}
