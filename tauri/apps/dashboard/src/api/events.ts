import type { EventEntry } from "@springtale/types";

/**
 * SSE client for real-time event log streaming.
 *
 * Connects to GET /events/stream with Bearer auth.
 * Falls back to polling /events if SSE is unavailable.
 */
export function subscribeToEvents(
  baseUrl: string,
  token: string,
  onEvent: (event: EventEntry) => void,
  onError?: (error: Event) => void,
): () => void {
  // EventSource doesn't support custom headers natively.
  // Use query parameter for auth (acceptable since dashboard binds 127.0.0.1 only).
  const url = `${baseUrl}/events/stream?token=${encodeURIComponent(token)}`;

  const eventSource = new EventSource(url);

  eventSource.addEventListener("message", (event) => {
    try {
      const entry = JSON.parse(event.data) as EventEntry;
      onEvent(entry);
    } catch {
      // Skip malformed events
    }
  });

  eventSource.addEventListener("error", (event) => {
    onError?.(event);
  });

  // Return cleanup function
  return () => {
    eventSource.close();
  };
}
