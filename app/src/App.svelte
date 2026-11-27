<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./components/Icon.svelte";
  import mark from "./assets/mark.svg";
  import { api, on } from "./lib/api";
  import { applyTheme, go, refreshInbox, t, ui, type Page } from "./lib/state.svelte";
  import Home from "./pages/Home.svelte";
  import Clean from "./pages/Clean.svelte";
  import Startup from "./pages/Startup.svelte";
  import Health from "./pages/Health.svelte";
  import Assistant from "./pages/Assistant.svelte";
  import Inbox from "./pages/Inbox.svelte";
  import History from "./pages/History.svelte";
  import SettingsPage from "./pages/Settings.svelte";

  const nav = $derived<{ id: Page; label: string; icon: string }[]>([
    { id: "home", label: t("Home", "Home"), icon: "home" },
    { id: "clean", label: t("Clean", "Safai"), icon: "broom" },
    { id: "startup", label: t("Startup", "Startup"), icon: "rocket" },
    { id: "health", label: t("Health", "Sehat"), icon: "pulse" },
    { id: "assistant", label: t("Assistant", "Assistant"), icon: "chat" },
    { id: "inbox", label: t("Messages", "Messages"), icon: "inbox" },
    { id: "history", label: t("History", "Hisaab"), icon: "history" },
    { id: "settings", label: t("Settings", "Settings"), icon: "settings" },
  ]);

  onMount(() => {
    try {
      const t = localStorage.getItem("theme");
      if (t === "light" || t === "dark") ui.theme = t;
    } catch { /* ignore */ }
    api.appInfo().then((i) => (ui.info = i));
    api.settings().then((s) => (ui.settings = s));
    refreshInbox();
    const unsubs: Promise<() => void>[] = [
      on<string>("navigate", (p) => go(p as Page)),
      on("inbox-changed", () => refreshInbox()),
    ];
    const timer = setInterval(refreshInbox, 60000);
    return () => { clearInterval(timer); unsubs.forEach((u) => u.then((f) => f())); };
  });

  function cycleTheme() {
    applyTheme(ui.theme === "system" ? "dark" : ui.theme === "dark" ? "light" : "system");
  }
</script>

<div class="shell">
  <aside>
    <div class="brand">
      <img src={mark} alt="" width="34" height="34" />
      <div><strong>Sweep<span>lark</span></strong><small>Small bird. Clean PC.</small></div>
    </div>
    <nav>
      {#each nav as n}
        <button class="nav-item" class:active={ui.page === n.id} onclick={() => go(n.id)}>
          <Icon name={n.icon} size={19} />
          <span>{n.label}</span>
          {#if n.id === "inbox" && ui.pendingApprovals + ui.unread > 0}
            <em class:amber={ui.pendingApprovals > 0}>{ui.pendingApprovals || ui.unread}</em>
          {/if}
        </button>
      {/each}
    </nav>
    <div class="foot">
      <div class="local"><Icon name="lock" size={14} /><span>{t("Offline · no tracking", "Offline · koi tracking nahi")}</span></div>
      <div class="row">
        <span class="muted small">v{ui.info?.version ?? "1.1.0"}</span>
        <span class="spacer"></span>
        <button class="btn ghost small" onclick={cycleTheme} title="Theme: {ui.theme}" aria-label={t("Change theme", "Theme badlein")}>
          <Icon name={ui.theme === "dark" ? "moon" : ui.theme === "light" ? "sun" : "sparkle"} size={16} />
        </button>
      </div>
    </div>
  </aside>

  <main>
    {#key ui.page}
      <div class="page fade">
        {#if ui.page === "home"}<Home />
        {:else if ui.page === "clean"}<Clean />
        {:else if ui.page === "startup"}<Startup />
        {:else if ui.page === "health"}<Health />
        {:else if ui.page === "assistant"}<Assistant />
        {:else if ui.page === "inbox"}<Inbox />
        {:else if ui.page === "history"}<History />
        {:else}<SettingsPage />{/if}
      </div>
    {/key}
  </main>

  <div class="toasts" aria-live="polite">
    {#each ui.toasts as t (t.id)}
      <div class="toast fade {t.tone}"><Icon name={t.tone === "good" ? "check" : t.tone === "warn" ? "alert" : "info"} size={17} />{t.text}</div>
    {/each}
  </div>
</div>

<style>
  .shell { display: grid; grid-template-columns: 232px 1fr; height: 100vh; }
  aside {
    display: flex; flex-direction: column; gap: 18px; padding: 20px 14px 14px;
    background: var(--bg); border-right: 1px solid var(--line);
  }
  .brand { display: flex; align-items: center; gap: 10px; padding: 0 8px 4px; }
  .brand strong { display: block; font-size: 21px; font-weight: 700; letter-spacing: -0.02em; line-height: 1.05; }
  .brand strong span { color: var(--teal); }
  .brand small { color: var(--slate); font-size: 11.5px; }
  nav { display: flex; flex-direction: column; gap: 2px; }
  .nav-item {
    display: flex; align-items: center; gap: 12px; height: 40px; padding: 0 12px; border: 0; border-radius: 12px;
    background: transparent; color: var(--ink-2); cursor: pointer; font-weight: 520; text-align: left; position: relative;
  }
  .nav-item:hover { background: var(--surface-3); }
  .nav-item { border: 1px solid transparent; }
  .nav-item.active { background: var(--surface); color: var(--ink); font-weight: 600; box-shadow: var(--shadow); border: 1px solid var(--line); }
  .nav-item.active :global(svg) { color: var(--teal); }
  .nav-item em { margin-left: auto; font-style: normal; font-size: 11px; font-weight: 700; min-width: 20px; height: 20px; padding: 0 6px; border-radius: 999px; display: grid; place-items: center; background: var(--teal); color: #fff; }
  .nav-item em.amber { background: var(--amber); color: #3a2500; }
  .foot { margin-top: auto; display: flex; flex-direction: column; gap: 8px; padding: 0 6px; }
  .local { display: flex; align-items: center; gap: 7px; font-size: 12px; color: var(--slate); padding: 4px 6px; }
  main {
    overflow: auto; position: relative;
    background: var(--surface-2);
  }
  .page { max-width: 1080px; margin: 0 auto; padding: 32px 36px 48px; }
  .toasts { position: fixed; right: 20px; bottom: 20px; display: flex; flex-direction: column; gap: 8px; z-index: 60; }
  .toast { display: flex; align-items: center; gap: 9px; padding: 11px 16px; border-radius: 12px; background: var(--ink); color: var(--bg); box-shadow: var(--shadow-lg); font-weight: 520; max-width: 420px; }
  .toast.good :global(svg) { color: var(--mint); }
  .toast.warn :global(svg) { color: var(--amber); }
</style>
