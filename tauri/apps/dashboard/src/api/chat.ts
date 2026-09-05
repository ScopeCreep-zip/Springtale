import type { ChatStreamMessage } from "@springtale/ui";
import { type SseState, subscribeSse } from "./sse";

/**
 * In-app chat client (W5).
 *
 * `sendChatMessage` POSTs to /chat (fire-and-forget; the bot replies
 * asynchronously). `subscribeToChat` opens an SSE stream on /chat/stream and
 * invokes the callback for each reply. The stream is opened with a
 * one-time ticket (see `sse.ts`) — never the bearer token in the URL.
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
  onMessage: (message: ChatStreamMessage) => void,
  onState?: (s: SseState) => void,
): () => void {
  return subscribeSse(
    "/chat/stream",
    (name, data) => {
      if (name === "message") onMessage(data as ChatStreamMessage);
    },
    onState,
    baseUrl,
  );
}
