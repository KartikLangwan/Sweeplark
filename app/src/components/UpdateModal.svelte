<script lang="ts">
  import { onMount } from "svelte";
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";
  import { api, on } from "../lib/api";
  import { bytes } from "../lib/format";
  import { t, toast, ui } from "../lib/state.svelte";
  import type { UpdateInfo } from "../lib/types";

  let info = $state<UpdateInfo | null>(null);
  let open = $state(false);
  let busy = $state(false);
  let progress = $state<[number, number] | null>(null);

  onMount(() => {
    api.updateCheck(false).then((u) => { if (u) { info = u; open = true; } }).catch(() => {});
    const a = on<UpdateInfo>("update-available", (u) => { info = u; open = true; });
    const b = on<[number, number]>("update-progress", (p) => (progress = p));
    const local = (e: Event) => { info = (e as CustomEvent<UpdateInfo>).detail; open = true; };
    window.addEventListener("sweeplark-update", local);
    return () => { a.then((f) => f()); b.then((f) => f()); window.removeEventListener("sweeplark-update", local); };
  });

  async function install() {
    busy = true;
    try { await api.updateInstall(); } catch (e) { toast(String(e), "warn"); busy = false; }
  }
  async function skip() {
    if (info) await api.updateSkip(info.version);
    open = false;
  }
</script>

<Modal bind:open title={t("Update available", "Naya update aaya hai")}>
  {#if info}
    <div class="col">
      <div class="row"><span class="badge"><Icon name="download" /></span>
        <div class="col" style="gap:2px"><strong>Sweeplark {info.version}</strong>
          <span class="muted small">{t("You have", "Aapke paas")} {ui.info?.version}{info.setup_size ? ` · ${bytes(info.setup_size)}` : ""}</span></div></div>
      {#if info.notes}<pre class="notes selectable">{info.notes}</pre>{/if}
      {#if progress}<div class="bar"><span style="width:{progress[1] ? (progress[0] / progress[1]) * 100 : 5}%"></span></div>{/if}
      <p class="muted small">{ui.info?.portable
        ? t("This is the portable copy: the download page will open.", "Yeh portable copy hai: download page khulega.")
        : t("The update is checked, installed, and Sweeplark restarts. Your settings stay.", "Update check hokar install hoga aur Sweeplark dobara khulega. Settings wahi rahengi.")}</p>
    </div>
  {/if}
  {#snippet footer()}
    <button class="btn ghost" onclick={skip} disabled={busy}>{t("Skip this version", "Is version ko chhodo")}</button>
    <button class="btn" onclick={() => (open = false)} disabled={busy}>{t("Later", "Baad mein")}</button>
    <button class="btn primary" onclick={install} disabled={busy}>{busy ? t("Updating…", "Update ho raha hai…") : t("Update now", "Abhi update karo")}</button>
  {/snippet}
</Modal>

<style>
  .badge { width: 40px; height: 40px; border-radius: 12px; display: grid; place-items: center; background: var(--teal-soft); color: var(--teal-text); }
  .notes { font-family: inherit; white-space: pre-wrap; margin: 0; padding: 12px; border-radius: 10px; background: var(--surface-2); border: 1px solid var(--line); font-size: 13px; max-height: 180px; overflow: auto; }
</style>
