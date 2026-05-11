/**
 * Canvas IPC wrappers — fetch state + subscribe to live updates for
 * the A2UI surface the bot pushes content to.
 *
 * Live updates use `tauri::ipc::Channel<CanvasUpdate>` via the
 * `subscribe_canvas` command (per Phase E10 / F4). Channel beats
 * `emit()` for streaming throughput. `onCanvasUpdate` is the
 * page-level convenience wrapper around that channel — desktop's
 * `Canvas` page uses it to render bot-pushed blocks live.
 */
import { invoke, Channel } from "@tauri-apps/api/core";
import type { CanvasState, CanvasUpdate } from "@springtale/types";

export interface ConnectionPipe {
  id: string;
  dir: 1 | -1;
  status: string;
}

export interface ConnectionData {
  a: string;
  b: string;
  pipes: ConnectionPipe[];
}

export async function getConnections(): Promise<ConnectionData[]> {
  return invoke<ConnectionData[]>("get_connections");
}

export async function getCanvasState(): Promise<CanvasState> {
  return invoke<CanvasState>("get_canvas_state");
}

/**
 * Subscribe to canvas updates via the `tauri::ipc::Channel<CanvasUpdate>`
 * IPC primitive registered server-side by `subscribe_canvas`. The
 * callback fires once per backend-published update; the returned
 * disposer drops the channel.
 *
 * Channels are per-window — each call gets a fresh handler that the
 * backend appends to its registry. Calling this multiple times in
 * the same window subscribes that many independent listeners.
 */
export async function onCanvasUpdate(
  callback: (update: CanvasUpdate) => void,
): Promise<() => void> {
  const channel = new Channel<CanvasUpdate>();
  channel.onmessage = callback;
  await invoke("subscribe_canvas", { channel });
  return () => {
    // Channels are dropped server-side when the Window closes; the
    // forwarder task exits on the next send-error. There's no
    // explicit unsubscribe in Tauri 2's Channel API yet, so the
    // disposer is a no-op until that lands. Documented so callers
    // know not to rely on synchronous teardown.
  };
}
