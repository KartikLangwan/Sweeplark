// The assistant's conversation lives here, so a question keeps being answered (and the
// chat stays) when the user opens another page.

import { api } from "./api";
import { t } from "./state.svelte";
import type { AssistantReply, ChatTurn } from "./types";

export type Msg = { role: "user" | "assistant"; text: string; reply?: AssistantReply; status?: string; pending?: boolean };

export const chat = $state({ msgs: [] as Msg[], busy: false });

export async function ask(q: string, onUpdate?: () => void) {
  q = q.trim();
  if (!q || chat.busy) return;
  chat.busy = true;
  const history: ChatTurn[] = chat.msgs.filter((m) => !m.pending).map((m) => ({ role: m.role, content: m.text }));
  chat.msgs.push({ role: "user", text: q });
  chat.msgs.push({ role: "assistant", text: "", pending: true, status: t("Thinking…", "Soch raha hoon…") });
  const idx = chat.msgs.length - 1;
  onUpdate?.();
  try {
    const reply = await api.chat(q, history.slice(-6), (e) => {
      if (e.event === "status") chat.msgs[idx].status = e.data;
      else { chat.msgs[idx].text += e.data; chat.msgs[idx].status = undefined; onUpdate?.(); }
    });
    chat.msgs[idx] = { role: "assistant", text: reply.text, reply };
  } catch (e) {
    chat.msgs[idx] = { role: "assistant", text: t(`Sorry, I couldn't answer: ${e}`, `Maaf kijiye, jawab nahi bana: ${e}`) };
  } finally {
    chat.busy = false;
    onUpdate?.();
  }
}

export function clearChat() {
  if (!chat.busy) chat.msgs = [];
}
