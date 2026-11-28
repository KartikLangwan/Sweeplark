<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "../components/Icon.svelte";
  import FindingCard from "../components/FindingCard.svelte";
  import mark from "../assets/mark.svg";
  import { api } from "../lib/api";
  import { actionIcon, actionLabel, runAction } from "../lib/actions";
  import { ask, chat, clearChat } from "../lib/chat.svelte";
  import { go, ui , t } from "../lib/state.svelte";
  import type { AiStatus } from "../lib/types";

  let input = $state("");
  let status = $state<AiStatus | null>(null);
  let scroller: HTMLDivElement;
  const msgs = $derived(chat.msgs);
  const busy = $derived(chat.busy);

  const suggestions = $derived(ui.settings?.language === "hinglish"
    ? ["Mera PC slow kyun hai?", "RAM kaise khaali karoon?", "Kaunse optimizations baaki hain?", "C drive kyun bhar rahi hai?", "Sabse badi apps kaunsi hain?", "Mera GPU kitna use ho raha hai?", "Kya mera PC virus se safe hai?", "Aaj tumne kya kiya?"]
    : ["Why is my PC slow?", "How do I free up RAM?", "Which optimizations are still off?", "Why is my C drive filling up?", "What are my biggest apps?", "How busy is my graphics card?", "Is my PC protected from viruses?", "What did you do today?"]);

  onMount(() => { api.aiStatus().then((s) => (status = s)); scrollDown(); });

  const modelName = $derived(status?.models.find((m) => m.tier === status?.tier)?.name);
  const aiReady = $derived(!!status?.enabled && !!status.tier && status.install.installed_tiers.includes(status.tier) && status.install.runtime_installed);

  async function scrollDown() {
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" });
  }

  async function send(q = input) {
    if (!q.trim() || busy) return;
    input = "";
    await ask(q, scrollDown);
    api.aiStatus().then((s) => (status = s));
  }
</script>

<div class="wrap">
  <header class="head">
    <div><h1>Assistant</h1>
      <p class="muted row small">
        <Icon name="lock" size={14} />{t("Fully offline", "Poori tarah offline")} ·
        {#if aiReady}{modelName} · {status?.loaded ? t("loaded now", "abhi load hai") : t("loads when needed", "zarurat par load hoga")}{:else}{t("No AI model installed: you get the app's own answers", "AI model nahi laga: app ke apne jawab milenge")}{/if}
      </p>
    </div>
    <div class="row">
      {#if msgs.length}<button class="btn ghost small" onclick={clearChat} disabled={busy}><Icon name="restart" size={14} />{t("New chat", "Nayi baat")}</button>{/if}
      {#if !aiReady}<button class="btn" onclick={() => go("settings")}><Icon name="download" size={17} />{t("Install AI model", "AI model lagayein")}</button>{/if}
    </div>
  </header>

  <div class="chat card" bind:this={scroller}>
    {#if !msgs.length}
      <div class="welcome">
        <img src={mark} alt="" width="64" height="64" />
        <h2>{t(`I'm ${ui.info?.name ?? "Sweeplark"}. Ask me anything about your PC.`, `Main ${ui.info?.name ?? "Sweeplark"} hoon. PC ke baare mein kuch bhi poochiye.`)}</h2>
        <p class="muted">{t("I answer from your PC's real data: RAM, CPU, graphics card, disks, background apps, startup, Optimizer tweaks, installed apps and security. I never change anything myself; I suggest and give you a button to do it.", "Main aapke PC ki asli jaankari dekh kar jawab deta hoon: RAM, CPU, graphics card, disk, background apps, startup, Optimizer tweaks, installed apps aur security. Main khud kuch nahi badalta; salah aur button deta hoon.")}</p>
        <div class="sugs">{#each suggestions as s}<button class="btn small" onclick={() => send(s)}>{s}</button>{/each}</div>
      </div>
    {/if}
    {#each msgs as m}
      {#if m.role === "user"}
        <div class="bubble user selectable fade">{m.text}</div>
      {:else}
        <div class="ai fade">
          <img src={mark} alt="" width="30" height="30" class="avatar" />
          <div class="col grow" style="gap:8px">
            {#if m.pending && !m.text}<div class="row muted small"><div class="spin"></div>{m.status}</div>{/if}
            {#if m.text}<div class="bubble bot selectable">{m.text}</div>{/if}
            {#if m.reply}
              {#if m.reply.key_points.length && m.reply.source === "model"}
                <div class="col points">{#each m.reply.key_points.slice(0, 3) as f}<FindingCard finding={f} compact />{/each}</div>
              {/if}
              {#if m.reply.actions.length}
                <div class="row wrap acts">{#each m.reply.actions as a}<button class="btn small" onclick={() => runAction(a)}><Icon name={actionIcon(a)} size={14} />{actionLabel(a)}</button>{/each}</div>
              {/if}
              <span class="meta">{m.reply.source === "model" ? t(`Written by ${m.reply.model} · `, `${m.reply.model} ne likha · `) : t("App answer · ", "App ka jawab · ")}{t("checked:", "dekha:")} {m.reply.tools.join(", ")}</span>
            {/if}
          </div>
        </div>
      {/if}
    {/each}
  </div>

  <form class="composer card" onsubmit={(e) => { e.preventDefault(); send(); }}>
    <input type="text" bind:value={input} placeholder={t("e.g. Why is my PC getting hot?", "Jaise: mera PC garam kyun ho raha hai?")} disabled={busy} aria-label={t("Question", "Sawaal")} />
    <button class="btn primary" type="submit" disabled={busy || !input.trim()} aria-label={t("Send", "Bhejein")}><Icon name="send" size={18} /></button>
  </form>
</div>

<style>
  .wrap { display: flex; flex-direction: column; height: calc(100vh - 68px); }
  .head { display: flex; align-items: flex-end; justify-content: space-between; margin-bottom: 14px; }
  .chat { flex: 1; overflow: auto; display: flex; flex-direction: column; gap: 14px; padding: 22px; }
  .welcome { margin: auto; max-width: 560px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .sugs { display: flex; flex-wrap: wrap; justify-content: center; gap: 8px; margin-top: 6px; }
  .bubble { padding: 11px 15px; border-radius: 16px; max-width: 78%; white-space: pre-wrap; line-height: 1.5; }
  .bubble.user { align-self: flex-end; background: var(--teal); color: #fff; border-bottom-right-radius: 5px; }
  .bubble.bot { background: var(--surface-3); border-bottom-left-radius: 5px; max-width: 100%; }
  .ai { display: flex; gap: 10px; align-items: flex-start; max-width: 86%; }
  .avatar { flex: none; margin-top: 2px; }
  .grow { flex: 1; min-width: 0; }
  .meta { font-size: 11.5px; color: var(--slate); }
  .acts { gap: 6px; }
  .composer { display: flex; gap: 10px; padding: 10px; margin-top: 12px; }
  .composer input { flex: 1; border: 0; background: transparent; height: 40px; }
  .composer input:focus-visible { outline: none; }
</style>
