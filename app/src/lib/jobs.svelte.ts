// Long tasks (scan, storage analysis, duplicates, downloads ...) live here, not inside a page,
// so they keep running and keep their results when the user opens another page.

import { on } from "./api";
import { go, t, toast, ui, type Page } from "./state.svelte";
import type {
  AppUpdate, BoostPlan, BoostReport, CleanReport, DuplicateReport, InstallProgress, InstalledApp, JobProgress, OptimizePlan,
  OptimizeReport, ScanReport, StorageReport,
} from "./types";

export type JobId = "scan" | "clean" | "storage" | "duplicates" | "tweaks" | "optimize" | "apps" | "updates" | "boost" | "ai";

export interface Job {
  id: JobId;
  label: string;
  page: Page;
  running: boolean;
  done: number;
  total: number;
  text: string;
  started: number;
}

export const jobs = $state<Partial<Record<JobId, Job>>>({});

/** Results that survive page changes (until the window is closed). */
export const results = $state({
  scan: null as ScanReport | null,
  clean: null as CleanReport | null,
  storage: null as StorageReport | null,
  duplicates: null as DuplicateReport | null,
  plan: null as OptimizePlan | null,
  optimized: null as OptimizeReport | null,
  apps: null as InstalledApp[] | null,
  updates: null as AppUpdate[] | null,
  boostPlan: null as BoostPlan | null,
  boost: null as BoostReport | null,
  /** Ticks on result lists (cleaners, files), kept with the results. */
  sel: {} as Record<string, Record<string, boolean>>,
  /** One-Click Optimize step on the Home page. */
  homePhase: "idle" as "idle" | "scanning" | "planned" | "fixing" | "done",
});

export const running = (id: JobId) => !!jobs[id]?.running;

/** Run `fn` as a named job. A second start while it runs is ignored. */
export async function runJob<T>(id: JobId, label: string, page: Page, fn: () => Promise<T>): Promise<T | undefined> {
  if (jobs[id]?.running) return undefined;
  jobs[id] = { id, label, page, running: true, done: 0, total: 0, text: "", started: Date.now() };
  try {
    const r = await fn();
    if (ui.page !== page) toast(t(`${label}: done`, `${label}: ho gaya`), "good");
    return r;
  } catch (e) {
    toast(String(e), "warn");
    return undefined;
  } finally {
    delete jobs[id];
  }
}

/** Percentage 0-100, or null when the total is not known yet. */
export function percent(j: Job): number | null {
  if (!j.total) return null;
  return Math.min(100, Math.max(0, (j.done / j.total) * 100));
}

let listening = false;
export function listenJobs() {
  if (listening) return;
  listening = true;
  on<JobProgress>("job-progress", (p) => {
    const j = jobs[p.job as JobId];
    if (j?.running) {
      j.done = p.done;
      j.total = p.total;
      j.text = p.text;
    }
  });
  // The AI download runs in the app itself; show it like any other job.
  on<InstallProgress>("ai-progress", (p) => {
    if (p.stage === "runtime" || p.stage === "model") {
      const label = p.stage === "runtime" ? t("Downloading AI engine", "AI engine download") : t("Downloading AI model", "AI model download");
      const j = jobs.ai ?? (jobs.ai = { id: "ai", label, page: "settings", running: true, done: 0, total: 0, text: "", started: Date.now() });
      j.label = label;
      j.done = p.done;
      j.total = p.total;
    } else {
      delete jobs.ai;
    }
  });
}

export function openJob(j: Job) {
  go(j.page);
}
