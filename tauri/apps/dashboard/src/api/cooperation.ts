import type { CooperationEventEnvelope } from "@springtale/ui";

/**
 * Phase H — SSE client for the cooperation events stream.
 *
 * Verbatim mirror of `subscribeToEvents` (api/events.ts) but consumes
 * GET `/cooperation/events`. The web dashboard uses this; desktop uses
 * the Tauri `subscribe_cooperation` Channel<CooperationEventEnvelope>
 * via `apps/desktop/src/provider.ts` instead (per E10 — Channel<T> beats
 * SSE on Tauri for high-rate streams).
 */
export function subscribeToCooperationEvents(
  baseUrl: string,
  token: string,
  onEvent: (envelope: CooperationEventEnvelope) => void,
  onError?: (error: Event) => void,
): () => void {
  const url = `${baseUrl}/cooperation/events?token=${encodeURIComponent(token)}`;
  const eventSource = new EventSource(url);

  eventSource.addEventListener("message", (event) => {
    try {
      const envelope = JSON.parse(event.data) as CooperationEventEnvelope;
      onEvent(envelope);
    } catch {
      // Skip malformed envelopes — lag drops produce empty frames per
      // the SSE handler's filter_map(None) path.
    }
  });

  eventSource.addEventListener("error", (event) => {
    onError?.(event);
  });

  return () => {
    eventSource.close();
  };
}
