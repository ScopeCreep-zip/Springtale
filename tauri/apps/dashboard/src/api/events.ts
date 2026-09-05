import type { EventEntry } from "@springtale/types";
import { type SseState, subscribeSse } from "./sse";

/**
 * Live event-log entries — the `event` frames of the multiplexed
 * `GET /stream` (ticket-authenticated; see `sse.ts`).
 */
export function subscribeToEvents(
  baseUrl: string,
  onEvent: (event: EventEntry) => void,
  onState?: (s: SseState) => void,
): () => void {
  return subscribeSse(
    "/stream",
    (name, data) => {
      if (name === "event") onEvent(data as EventEntry);
    },
    onState,
    baseUrl,
  );
}
