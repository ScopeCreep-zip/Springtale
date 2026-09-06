/**
 * Track D one-click Onboard over HTTP (plan 2.5).
 *
 * `POST /workspaces/onboard` is an SSE stream behind the one-time
 * stream ticket (plan 0.7). The connector config rides in the POST
 * body — never the URL — so EventSource (GET-only) can't be used;
 * the frames are read from a fetch body instead.
 */

import type { ChatDiscoveredEvent } from "@springtale/ui";
import { getBaseUrl, post } from "./client";

const EVENT_NAME = "chat-discovered";

const sessions = new Map<string, AbortController>();
const listeners = new Set<(event: ChatDiscoveredEvent) => void>();

function dispatch(raw: string): void {
  let data: ChatDiscoveredEvent;
  try {
    data = JSON.parse(raw) as ChatDiscoveredEvent;
  } catch {
    return; // skip malformed frames
  }
  for (const listener of listeners) listener(data);
}

/** Handle one SSE frame block (`event: x` / `data: y` lines). */
function handleFrame(block: string): void {
  let name = "message";
  const data: string[] = [];
  for (const line of block.split("\n")) {
    if (line.startsWith("event:")) name = line.slice(6).trim();
    else if (line.startsWith("data:")) data.push(line.slice(5).trimStart());
  }
  if (name === EVENT_NAME && data.length > 0) dispatch(data.join("\n"));
}

async function readSse(body: ReadableStream<Uint8Array>, signal: AbortSignal): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  try {
    while (!signal.aborted) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let idx = buffer.indexOf("\n\n");
      while (idx >= 0) {
        handleFrame(buffer.slice(0, idx));
        buffer = buffer.slice(idx + 2);
        idx = buffer.indexOf("\n\n");
      }
    }
  } finally {
    reader.releaseLock();
  }
}

export function cancelOnboardStream(sessionId: string): void {
  const controller = sessions.get(sessionId);
  if (!controller) return;
  sessions.delete(sessionId);
  controller.abort();
}

export async function startOnboardStream(
  sessionId: string,
  connectorName: string,
  config: Record<string, unknown>,
  payload?: string,
): Promise<void> {
  // Replace any prior session under this id — same as the desktop.
  cancelOnboardStream(sessionId);
  const controller = new AbortController();
  sessions.set(sessionId, controller);

  const { ticket } = await post<{ ticket: string }>("/stream/ticket", {});
  const url = `${getBaseUrl()}/workspaces/onboard?ticket=${encodeURIComponent(ticket)}`;
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "text/event-stream" },
    body: JSON.stringify({
      session_id: sessionId,
      connector_name: connectorName,
      config,
      payload,
    }),
    signal: controller.signal,
  });
  if (!res.ok || !res.body) {
    sessions.delete(sessionId);
    throw new Error(`onboard stream failed: HTTP ${res.status}`);
  }
  void readSse(res.body, controller.signal)
    .catch(() => undefined)
    .finally(() => {
      if (sessions.get(sessionId) === controller) sessions.delete(sessionId);
    });
}

export function subscribeToChatDiscovered(
  callback: (event: ChatDiscoveredEvent) => void,
): () => void {
  listeners.add(callback);
  return () => {
    listeners.delete(callback);
  };
}
