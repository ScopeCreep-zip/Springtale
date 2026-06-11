/**
 * Canvas API client — fetches state and subscribes to SSE updates.
 *
 * Dashboard uses SSE (same pattern as events.ts).
 * Desktop uses Tauri events instead.
 */
import type { CanvasState, CanvasUpdate } from "@springtale/types";
import { get } from "./client";

export async function getCanvasState(): Promise<CanvasState> {
  return get<CanvasState>("/canvas");
}

export function subscribeToCanvasUpdates(
  baseUrl: string,
  token: string,
  onUpdate: (update: CanvasUpdate) => void,
  onDisconnect?: () => void,
): () => void {
  const url = `${baseUrl}/canvas/stream?token=${encodeURIComponent(token)}`;
  const eventSource = new EventSource(url);

  eventSource.addEventListener("message", (event) => {
    try {
      const update = JSON.parse(event.data) as CanvasUpdate;
      onUpdate(update);
    } catch {
      // Invalid JSON — skip
    }
  });

  eventSource.addEventListener("error", () => {
    onDisconnect?.();
  });

  return () => eventSource.close();
}
