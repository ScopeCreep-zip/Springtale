/**
 * In-app chat panel (W5) — the desktop / web / mobile-PWA surface for
 * talking to the bot directly, the same bot the connectors drive.
 *
 * Thin per the frontend rules: all behaviour goes through
 * `db.provider.sendChatMessage` / `subscribeToChat` (no raw fetch/invoke).
 * The bot does the work in the Rust backend — read-only tasks answer
 * immediately (W1); mutating ones pause for approval, surfaced through the
 * existing approvals UI (W2). This panel only presents the conversation.
 *
 * Mobile-first: a single vertical column, full-width bubbles, a thumb-reach
 * input bar pinned to the bottom. The colony theme classes carry the look.
 */

import type { Component } from "solid-js";
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";

import { useDashboard } from "../dashboard/context";

interface ChatTurn {
  role: "user" | "bot";
  text: string;
}

export interface ChatPanelProps {
  /** Optional session id; defaults to the single local in-app session. */
  session?: string;
}

export const ChatPanel: Component<ChatPanelProps> = (props) => {
  const db = useDashboard();
  const [turns, setTurns] = createSignal<ChatTurn[]>([]);
  const [draft, setDraft] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Auto-scroll: follow the conversation, but stop if the user scrolls up to
  // read history (resume once they return near the bottom). Standard chat-UX
  // pattern — never yank the viewport away from someone reading older turns.
  let scrollEl: HTMLDivElement | undefined;
  const [pinned, setPinned] = createSignal(true);

  const nearBottom = () =>
    !scrollEl || scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 80;

  const scrollToBottom = () => {
    if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight;
  };

  // After any new turn renders, scroll to the bottom if we're following.
  createEffect(() => {
    turns(); // track
    if (pinned()) requestAnimationFrame(scrollToBottom);
  });

  onMount(() => {
    const unsub = db.provider.subscribeToChat((msg) => {
      setTurns((prev) => [...prev, { role: "bot", text: msg.text }]);
      // A bot reply may be a conversational deploy confirmation ("set up
      // your weather bot"). Re-fetch rules/connectors so a chat-deployed
      // bot's tree/agent sprite appears on the live canvas without a
      // manual refresh. Fire-and-forget so it never blocks rendering the
      // reply; SolidJS's fine-grained reactivity makes a no-change
      // refetch (an ordinary read-only chat) effectively free.
      void db.refresh();
    });
    onCleanup(unsub);
  });

  const send = async () => {
    const text = draft().trim();
    if (!text || sending()) return;
    setError(null);
    setSending(true);
    // Sending your own message always snaps you to the bottom.
    setPinned(true);
    setTurns((prev) => [...prev, { role: "user", text }]);
    setDraft("");
    try {
      await db.provider.sendChatMessage(text, props.session);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to send");
    } finally {
      setSending(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  };

  return (
    <div class="flex h-full flex-col bg-soil-deep">
      <div
        ref={scrollEl}
        onScroll={() => setPinned(nearBottom())}
        class="flex-1 overflow-y-auto p-3 space-y-2"
      >
        <Show when={turns().length === 0}>
          <p class="text-text-dim text-center mt-8">
            Ask me anything — get the weather, research a topic, scrape a page, or make a change.
            Reads are instant; changes ask you first.
          </p>
        </Show>
        <For each={turns()}>
          {(turn) => (
            <div class={turn.role === "user" ? "text-right" : "text-left"}>
              <span
                class={
                  turn.role === "user"
                    ? "inline-block rounded px-3 py-2 bg-soil-mid text-text-primary max-w-[85%] text-left"
                    : "inline-block rounded px-3 py-2 bg-soil-raised text-text-secondary max-w-[85%]"
                }
              >
                {turn.text}
              </span>
            </div>
          )}
        </For>
      </div>

      <Show when={error()}>
        <p class="text-status-warn px-3 py-1 text-sm">{error()}</p>
      </Show>

      <div class="flex gap-2 border-t border-soil-line p-2">
        <textarea
          class="flex-1 resize-none rounded bg-soil-mid px-3 py-2 text-text-primary"
          rows={1}
          placeholder="Message…"
          value={draft()}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={onKeyDown}
        />
        <button
          type="button"
          class="rounded bg-status-ok px-4 py-2 text-soil-deep font-bold disabled:opacity-50"
          disabled={sending() || draft().trim().length === 0}
          onClick={() => void send()}
        >
          Send
        </button>
      </div>
    </div>
  );
};
