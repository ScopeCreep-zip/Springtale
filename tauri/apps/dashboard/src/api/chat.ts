import type { ChatStreamMessage } from "@springtale/ui";

/**
 * In-app chat client (W5).
 *
 * `sendChatMessage` POSTs to /chat (fire-and-forget; the bot replies
 * asynchronously). `subscribeToChat` opens an SSE stream on /chat/stream and
 * invokes the callback for each reply. Auth uses the ?token= query param
 * (EventSource can't set headers; the daemon binds 127.0.0.1 only).
 */
export async function sendChatMessage(
  baseUrl: string,
  token: string,
  text: string,
  session?: string,
): Promise<void> {
  const res = await fetch(`${baseUrl}/chat`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ text, session }),
  });
  if (!res.ok && res.status !== 202) {
    throw new Error(`chat send failed: ${res.status}`);
  }
}

export function subscribeToChat(
  baseUrl: string,
  token: string,
  onMessage: (message: ChatStreamMessage) => void,
  onError?: (error: Event) => void,
): () => void {
  const url = `${baseUrl}/chat/stream?token=${encodeURIComponent(token)}`;
  const eventSource = new EventSource(url);

  eventSource.addEventListener("message", (event) => {
    try {
      onMessage(JSON.parse(event.data) as ChatStreamMessage);
    } catch {
      // Skip malformed events
    }
  });

  eventSource.addEventListener("error", (event) => {
    onError?.(event);
  });

  return () => {
    eventSource.close();
  };
}
