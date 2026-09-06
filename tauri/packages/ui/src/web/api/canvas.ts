/**
 * Canvas API client — fetches state and subscribes to live updates.
 *
 * Dashboard consumes the `canvas` frames of the multiplexed `GET /stream`
 * (ticket-authenticated; see `sse.ts`). Desktop uses Tauri events instead.
 */
import type { CanvasState, CanvasUpdate } from "@springtale/types";
import { get } from "./client";
import { type SseState, subscribeSse } from "./sse";

export async function getCanvasState(): Promise<CanvasState> {
  return get<CanvasState>("/canvas");
}

export function subscribeToCanvasUpdates(
  baseUrl: string,
  onUpdate: (update: CanvasUpdate) => void,
  onDisconnect?: () => void,
): () => void {
  return subscribeSse(
    "/stream",
    (name, data) => {
      if (name === "canvas") onUpdate(data as CanvasUpdate);
    },
    (s: SseState) => {
      if (s === "closed") onDisconnect?.();
    },
    baseUrl,
  );
}
