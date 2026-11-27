<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "../components/Icon.svelte";
  import FindingCard from "../components/FindingCard.svelte";
  import mark from "../assets/mark.svg";
  import { api } from "../lib/api";
  import { go, ui , t } from "../lib/state.svelte";
  import type { AiStatus, AssistantReply, ChatTurn } from "../lib/types";

  type Msg = { role: "user" | "assistant"; text: string; reply?: AssistantReply; status?: string; pending?: boolean };

  let msgs = $state<Msg[]>([]);
  let input = $state("");
  let busy = $state(false);
  let status = $state<AiStatus | null>(null);
  let scroller: HTMLDivElement;

  const suggestions = $derived(ui.settings?.language === "hinglish"
    ? ["Mera PC slow kyun hai?", "C drive kyun bhar rahi hai?", "Kya mera PC virus se safe hai?", "Battery kaisi hai?", "Aaj tumne kya kiya?", "Tum kaun ho?"]
    : ["Why is my PC slow?", "Why is my C drive filling up?", "Is my PC protected from viruses?", "How is my battery?", "What did you do today?", "Who are you?"]);

  onMount(() => { api.aiStatus().then((s) => (status = s)); });

  const modelName = $derived(status?.models.find((m) => m.tier === status?.tier)?.name);
  const aiReady = $derived(!!status?.enabled && !!status.tier && status.install.installed_tiers.includes(status.tier) && status.install.runtime_installed);

  async function scrollDown() {
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight, behavior: "smooth" });
  }

  async function send(q = input) {
    q = q.trim();
    if (!q || busy) return;
    input = "";
    busy = true;
    const history: ChatTurn[] = msgs.filter((m) => !m.pending).map((m) => ({ role: m.role, content: m.text }));
    msgs.push({ role: "user", text: q });
    msgs.push({ role: "assistant", text: "", pending: true, status: t("Thinking…", "Soch raha hoon…") });
    const idx = msgs.length - 1;
    scrollDown();
    try {
      const reply = await api.chat(q, history.slice(-6), (e) => {
        if (e.event === "status") msgs[idx].status = e.data;
        else { msgs[idx].text += e.data; msgs[idx].status = undefined; scrollDown(); }
      });
      msgs[idx] = { role: "assistant", text: reply.text, reply };
    } catch (e) {
      msgs[idx] = { role: "assistant", text: t(`Sorry, I couldn't answer: ${e}`, `Maaf kijiye, jawab nahi bana: ${e}`) };
    } finally {
      busy = false;
      scrollDown();
      api.aiStatus().then((s) => (status = s));
    }
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
    {#if !aiReady}<button class="btn" onclick={() => go("settings")}><Icon name="download" size={17} />{t("Install AI model", "AI model lagayein")}</button>{/if}
  </header>

  <div class="chat card" bind:this={scroller}>
    {#if !msgs.length}
      <div class="welcome">
        <img src={mark} alt="" width="64" height="64" />
        <h2>{t(`I'm ${ui.info?.name ?? "Sweeplark"}. Ask me anything about your PC.`, `Main ${ui.info?.name ?? "Sweeplark"} hoon. PC ke baare mein kuch bhi poochiye.`)}</h2>
        <p class="muted">{t("I answer from your PC's real data (RAM, disks, startup, security). I never delete or turn off anything myself; I only suggest.", "Main aapke PC ki asli jaankari (RAM, disk, startup, security) dekh kar jawab deta hoon. Main khud kuch delete ya band nahi karta: sirf salah deta hoon.")}</p>
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
  .composer { display: flex; gap: 10px; padding: 10px; margin-top: 12px; }
  .composer input { flex: 1; border: 0; background: transparent; height: 40px; }
  .composer input:focus-visible { outline: none; }
</style>
