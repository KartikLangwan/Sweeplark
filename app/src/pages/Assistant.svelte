<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "../components/Icon.svelte";
  import FindingCard from "../components/FindingCard.svelte";
  import mark from "../assets/mark.svg";
  import { api } from "../lib/api";
  import { go, ui } from "../lib/state.svelte";
  import type { AiStatus, AssistantReply, ChatTurn } from "../lib/types";

  type Msg = { role: "user" | "assistant"; text: string; reply?: AssistantReply; status?: string; pending?: boolean };

  let msgs = $state<Msg[]>([]);
  let input = $state("");
  let busy = $state(false);
  let status = $state<AiStatus | null>(null);
  let scroller: HTMLDivElement;

  const suggestions = ["Mera PC slow kyun hai?", "C drive kyun bhar rahi hai?", "Kya mera PC virus se safe hai?", "Battery kaisi hai?", "Aaj tumne kya kiya?", "Tum kaun ho?"];

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
    msgs.push({ role: "assistant", text: "", pending: true, status: "Soch raha hoon…" });
    const idx = msgs.length - 1;
    scrollDown();
    try {
      const reply = await api.chat(q, history.slice(-6), (e) => {
        if (e.event === "status") msgs[idx].status = e.data;
        else { msgs[idx].text += e.data; msgs[idx].status = undefined; scrollDown(); }
      });
      msgs[idx] = { role: "assistant", text: reply.text, reply };
    } catch (e) {
      msgs[idx] = { role: "assistant", text: `Maaf kijiye, jawab nahi bana: ${e}` };
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
        <Icon name="lock" size={14} />Poori tarah offline ·
        {#if aiReady}{modelName} · {status?.loaded ? "abhi load hai" : "zarurat par load hoga"}{:else}AI model nahi laga: app ke apne jawab milenge{/if}
      </p>
    </div>
    {#if !aiReady}<button class="btn" onclick={() => go("settings")}><Icon name="download" size={17} />AI model lagayein</button>{/if}
  </header>

  <div class="chat card" bind:this={scroller}>
    {#if !msgs.length}
      <div class="welcome">
        <img src={mark} alt="" width="64" height="64" />
        <h2>Main {ui.info?.name ?? "Sweeplark"} hoon. PC ke baare mein kuch bhi poochiye.</h2>
        <p class="muted">Main aapke PC ki asli jaankari (RAM, disk, startup, security) dekh kar jawab deta hoon. Main khud kuch delete ya band nahi karta: sirf salah deta hoon.</p>
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
              <span class="meta">{m.reply.source === "model" ? `${m.reply.model} ne likha · ` : "App ka jawab · "}dekha: {m.reply.tools.join(", ")}</span>
            {/if}
          </div>
        </div>
      {/if}
    {/each}
  </div>

  <form class="composer card" onsubmit={(e) => { e.preventDefault(); send(); }}>
    <input type="text" bind:value={input} placeholder="Jaise: mera PC garam kyun ho raha hai?" disabled={busy} aria-label="Sawaal" />
    <button class="btn primary" type="submit" disabled={busy || !input.trim()} aria-label="Bhejein"><Icon name="send" size={18} /></button>
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
  .bubble.bot { background: var(--surface-2); border: 1px solid var(--line); border-bottom-left-radius: 5px; max-width: 100%; }
  .ai { display: flex; gap: 10px; align-items: flex-start; max-width: 86%; }
  .avatar { flex: none; margin-top: 2px; }
  .grow { flex: 1; min-width: 0; }
  .meta { font-size: 11.5px; color: var(--slate); }
  .composer { display: flex; gap: 10px; padding: 10px; margin-top: 12px; }
  .composer input { flex: 1; border: 0; background: transparent; height: 40px; }
  .composer input:focus-visible { outline: none; }
</style>
