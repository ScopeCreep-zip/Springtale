import { getBaseUrl, post } from "./client";

/**
 * Shared SSE client (plan 0.7).
 *
 * `EventSource` cannot send headers, so each connection is opened with a
 * one-time 30 s ticket from `POST /stream/ticket` (bearer-authenticated)
 * instead of the bearer token itself — the token never lands in a URL,
 * browser history, or a proxy log. Ticket errors and dropped connections
 * reconnect with exponential backoff (1 s → 30 s), fetching a fresh
 * ticket each time.
 *
 * One `EventSource` per path is shared by every subscriber (ref-counted),
 * so the multiplexed `GET /stream` costs a tab exactly one of the
 * browser's ~6 per-origin SSE connections. Frames are dispatched by their
 * SSE `event:` name (`event`, `canvas`, `cooperation`, `approval`);
 * unnamed frames arrive as `"message"`.
 */
export type SseState = "open" | "closed";
export type SseListener = (name: string, data: unknown) => void;

const NAMED_EVENTS = ["event", "canvas", "cooperation"] as const;
const INITIAL_DELAY_MS = 1000;
const MAX_DELAY_MS = 30_000;

interface Connection {
  url: string;
  es: EventSource | null;
  closed: boolean;
  delay: number;
  timer: ReturnType<typeof setTimeout> | null;
  listeners: Set<SseListener>;
  stateListeners: Set<(s: SseState) => void>;
}

const connections = new Map<string, Connection>();

function dispatch(conn: Connection, name: string, raw: string): void {
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return; // skip malformed frames
  }
  for (const listener of conn.listeners) listener(name, data);
}

function notify(conn: Connection, state: SseState): void {
  for (const listener of conn.stateListeners) listener(state);
}

function scheduleReconnect(conn: Connection): void {
  if (conn.closed || conn.timer) return;
  conn.timer = setTimeout(() => {
    conn.timer = null;
    void connect(conn);
  }, conn.delay);
  conn.delay = Math.min(conn.delay * 2, MAX_DELAY_MS);
}

async function connect(conn: Connection): Promise<void> {
  if (conn.closed) return;
  let ticket: string;
  try {
    ({ ticket } = await post<{ ticket: string }>("/stream/ticket", {}));
  } catch {
    scheduleReconnect(conn);
    return;
  }
  if (conn.closed) return;

  const es = new EventSource(`${conn.url}?ticket=${encodeURIComponent(ticket)}`);
  conn.es = es;
  es.onopen = () => {
    conn.delay = INITIAL_DELAY_MS;
    notify(conn, "open");
  };
  es.onmessage = (e) => dispatch(conn, "message", e.data as string);
  for (const name of NAMED_EVENTS) {
    es.addEventListener(name, (e) => dispatch(conn, name, (e as MessageEvent).data as string));
  }
  es.onerror = () => {
    es.close();
    conn.es = null;
    notify(conn, "closed");
    scheduleReconnect(conn);
  };
}

/**
 * Subscribe to an SSE route. `onEvent(name, data)` receives every frame
 * with its SSE event name and parsed JSON payload. Returns an unsubscribe
 * function; the underlying connection closes when its last subscriber
 * leaves.
 */
export function subscribeSse(
  path: string,
  onEvent: SseListener,
  onState?: (s: SseState) => void,
  baseUrl: string = getBaseUrl(),
): () => void {
  const url = `${baseUrl}${path}`;
  const existing = connections.get(url);
  const conn: Connection = existing ?? {
    url,
    es: null,
    closed: false,
    delay: INITIAL_DELAY_MS,
    timer: null,
    listeners: new Set(),
    stateListeners: new Set(),
  };
  if (!existing) {
    connections.set(url, conn);
    void connect(conn);
  }

  conn.listeners.add(onEvent);
  if (onState) {
    conn.stateListeners.add(onState);
    if (conn.es?.readyState === EventSource.OPEN) onState("open");
  }

  return () => {
    conn.listeners.delete(onEvent);
    if (onState) conn.stateListeners.delete(onState);
    if (conn.listeners.size > 0) return;
    conn.closed = true;
    if (conn.timer) clearTimeout(conn.timer);
    conn.timer = null;
    conn.es?.close();
    conn.es = null;
    connections.delete(url);
  };
}
