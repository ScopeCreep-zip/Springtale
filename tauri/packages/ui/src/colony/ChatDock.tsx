/**
 * Floating chat dock — anchors the in-app `ChatPanel` to the bottom-left of
 * the canvas viewport, sitting just above the minimap (RTS comms-panel
 * placement). Collapsible: a compact "Ask" tab when closed, the full panel
 * when open. Pure presentation over the shared provider — the bot does the
 * work in the backend (desktop: embedded bot loop; web: POST /chat).
 *
 * The host must give the viewport `position: relative` so this absolute dock
 * anchors to it rather than the page.
 */

import type { Component } from "solid-js";
import { createSignal, Show } from "solid-js";

import { ChatPanel } from "./ChatPanel";

export interface ChatDockProps {
  /** Optional session id forwarded to the panel. */
  session?: string;
  /**
   * Controlled open state. When provided, the host owns the open/closed
   * state (e.g. so the command-grid "ASK" action can open the dock).
   * When omitted, the dock manages its own state from the tab click.
   */
  open?: boolean;
  /** Notified whenever the open state changes (controlled or self-toggle). */
  onOpenChange?: (open: boolean) => void;
}

export const ChatDock: Component<ChatDockProps> = (props) => {
  const [internalOpen, setInternalOpen] = createSignal(false);
  const open = () => props.open ?? internalOpen();
  const setOpen = (next: boolean) => {
    setInternalOpen(next);
    props.onOpenChange?.(next);
  };

  return (
    <div class="pointer-events-auto absolute bottom-2 left-2 z-20">
      <Show
        when={open()}
        fallback={
          <button
            type="button"
            class="colony-text-2xs flex items-center gap-1 rounded border-2 border-bark bg-soil-mid px-3 py-2 text-text-secondary hover:bg-soil-light"
            onClick={() => setOpen(true)}
            title="Ask Springtale"
          >
            <span class="colony-text-icon">?</span> ASK
          </button>
        }
      >
        <div class="flex h-[360px] w-[320px] flex-col overflow-hidden rounded border-2 border-bark bg-soil-mid shadow-lg">
          <div class="flex items-center justify-between border-b border-soil-line px-2 py-1">
            <span class="colony-text-2xs font-bold text-text-primary">Ask Springtale</span>
            <button
              type="button"
              class="colony-close-btn"
              onClick={() => setOpen(false)}
              title="Collapse"
            >
              ▾
            </button>
          </div>
          <div class="min-h-0 flex-1">
            <ChatPanel session={props.session} />
          </div>
        </div>
      </Show>
    </div>
  );
};
