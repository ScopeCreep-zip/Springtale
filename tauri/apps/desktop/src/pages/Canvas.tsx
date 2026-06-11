import type { CanvasState, CanvasUpdate } from "@springtale/types";
import { Canvas as CanvasComponent, useI18n } from "@springtale/ui";
import { createSignal, onCleanup, onMount } from "solid-js";
import { getCanvasState, onCanvasUpdate } from "../ipc/canvas";

/**
 * Canvas page — live UI surface the bot pushes content to.
 *
 * Per ARCHITECTURE.md: "A live UI surface that the agent can
 * programmatically push content to."
 *
 * Receives structured data via Tauri events. Never renders raw HTML.
 */
export function CanvasPage() {
  const { t } = useI18n();
  const [state, setState] = createSignal<CanvasState | null>(null);
  const [error, setError] = createSignal("");

  let unlisten: (() => void) | undefined;

  const applyUpdate = (current: CanvasState | null, update: CanvasUpdate): CanvasState => {
    const base: CanvasState = current ?? { blocks: [], updated_at: new Date().toISOString() };
    const blocks = [...base.blocks];

    switch (update.action) {
      case "SetBlocks":
        return { ...base, blocks: update.blocks, updated_at: new Date().toISOString() };
      case "UpdateBlock": {
        const idx = blocks.findIndex((b) => b.id === update.id);
        if (idx >= 0) {
          blocks[idx] = update.block;
        } else {
          blocks.push(update.block);
        }
        return { ...base, blocks, updated_at: new Date().toISOString() };
      }
      case "RemoveBlock":
        return {
          ...base,
          blocks: blocks.filter((b) => b.id !== update.id),
          updated_at: new Date().toISOString(),
        };
      case "Clear":
        return { ...base, blocks: [], updated_at: new Date().toISOString() };
      default:
        return base;
    }
  };

  onMount(async () => {
    try {
      const initial = await getCanvasState();
      setState(initial);
    } catch {
      // Canvas not initialized yet — start empty
    }

    try {
      unlisten = await onCanvasUpdate((update) => {
        setState((prev) => applyUpdate(prev, update));
      });
    } catch (e) {
      setError(String(e));
    }
  });

  onCleanup(() => {
    unlisten?.();
  });

  return (
    <div>
      <h1 class="text-2xl font-bold text-white">{t("canvas.title")}</h1>
      {error() && (
        <div
          role="alert"
          aria-live="assertive"
          class="mt-4 rounded border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400"
        >
          {error()}
        </div>
      )}
      <div class="mt-6">
        <CanvasComponent state={state()} />
      </div>
    </div>
  );
}
