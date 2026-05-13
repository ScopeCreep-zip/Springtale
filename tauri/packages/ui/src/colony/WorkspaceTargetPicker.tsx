/**
 * D1 — Workspace target picker for the recipe deploy form.
 *
 * Renders a dropdown over the formation's
 * `mental_model_workspaces` (filtered by the recipe's `connector`
 * hint), plus:
 *
 *   - 🔍 Scan — calls `discover_destinations` on the connector
 *     and refreshes the dropdown.
 *   - 🎯 Onboard — Telegram-specific affordance. Shows the
 *     `t.me/{bot}?start=…` deep link copy.
 *   - ✏️ Manual entry — text input + register via
 *     `upsertWorkspaceManual`. Power-user escape hatch.
 *
 * The picker resolves to the **workspace_key URI** (e.g.
 * `telegram://chat/12345`). Recipe TOML substitution then passes
 * the URI to the connector's `send_message`, which parses it via
 * `springtale-connector::workspace_key::extract_id_for_scheme`.
 * Existing recipes that pass raw IDs continue to work — the
 * parser falls back to raw-id semantics when no `://` is present.
 */
import { For, Show, createMemo, createResource, createSignal, onCleanup } from "solid-js";
import type { Component } from "solid-js";

import { useDashboard } from "../dashboard/context";
import type { ChatDiscoveredEvent, WorkspaceInfo } from "../dashboard/types";

export interface WorkspaceTargetPickerProps {
  /** The connector this destination belongs to. */
  connector: string;
  /** Optional `kind` filter — empty means "no filter". */
  kinds?: string[];
  /** Formation scoping — destinations are per-formation. */
  formationId: string;
  /** Currently-picked workspace_key URI (or raw id for legacy). */
  value: string;
  /** Called when the user picks / scans / clears the field. */
  onChange: (value: string) => void;
  /**
   * Sibling form inputs (the recipe deploy form's `inputs.values`).
   * The Onboard button forwards these to the runtime's connector
   * factory so the one-shot onboarding call has the credentials it
   * needs (e.g. `bot_token` for Telegram, `bot_token` + `app_id` for
   * Discord). Connector-agnostic — the runtime owns the dispatch.
   */
  formInputs?: Record<string, unknown>;
}

export const WorkspaceTargetPicker: Component<WorkspaceTargetPickerProps> = (
  props,
) => {
  const db = useDashboard();
  const [scanning, setScanning] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [manualOpen, setManualOpen] = createSignal(false);
  const [manualEntry, setManualEntry] = createSignal("");
  const [refreshTick, setRefreshTick] = createSignal(0);
  // Track D — transient discoveries from the live onboarding stream.
  // Keyed by `workspace_key` so the same chat can't show up twice
  // when the stream fires multiple events for it.
  const [liveDiscoveries, setLiveDiscoveries] = createSignal<
    Record<string, WorkspaceInfo>
  >({});

  const filterKey = () =>
    JSON.stringify({
      formationId: props.formationId,
      connector: props.connector,
      tick: refreshTick(),
    });

  const [workspaces] = createResource(filterKey, async () => {
    if (!props.formationId) return [] as WorkspaceInfo[];
    return db.provider.listWorkspaces(props.formationId, props.connector);
  });

  // Dropdown shows the union of persisted (`workspaces()`) and
  // transient live discoveries, deduplicated by workspace_key.
  // Live discoveries win because they're the freshest data.
  const filtered = createMemo<WorkspaceInfo[]>(() => {
    const persisted = workspaces() ?? [];
    const live = liveDiscoveries();
    const byKey = new Map<string, WorkspaceInfo>();
    for (const w of persisted) byKey.set(w.workspace_key, w);
    for (const k in live) {
      const v = live[k];
      if (v) byKey.set(k, v);
    }
    let rows = Array.from(byKey.values());
    if (props.kinds && props.kinds.length > 0) {
      const allow = new Set(props.kinds);
      rows = rows.filter((r) => allow.has(r.kind));
    }
    return rows;
  });

  const onScan = async () => {
    setError(null);
    setScanning(true);
    try {
      await db.provider.scanWorkspaces(props.formationId, props.connector);
      setRefreshTick((n) => n + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  const onCommitManual = async () => {
    const key = manualEntry().trim();
    if (!key) return;
    setError(null);
    try {
      await db.provider.upsertWorkspaceManual(
        props.formationId,
        key,
        // Use the raw id as the display name fallback for manual
        // entries — the user knows what they typed.
        key,
        props.connector,
        guessKindFromKey(key),
      );
      props.onChange(key);
      setRefreshTick((n) => n + 1);
      setManualOpen(false);
      setManualEntry("");
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div class="mt-1">
      <div class="flex items-center gap-2">
        <select
          class="colony-text-xs flex-1 border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
          value={props.value}
          onChange={(e) => props.onChange(e.currentTarget.value)}
        >
          <option value="">
            {workspaces.loading
              ? "Loading…"
              : filtered().length === 0
                ? "(no destinations yet — Scan or Onboard)"
                : "Pick a destination…"}
          </option>
          <For each={filtered()}>
            {(w) => (
              <option value={w.workspace_key}>
                {w.display_name} · {w.kind} · {relativeTime(w.last_seen_at_unix_ms)}
              </option>
            )}
          </For>
        </select>
      </div>

      <div class="mt-1 flex flex-wrap gap-1">
        <button
          type="button"
          class="colony-text-3xs rounded border border-bark bg-soil-mid px-2 py-1 hover:bg-soil-light disabled:opacity-50"
          onClick={onScan}
          disabled={scanning()}
          title="Ask the connector to enumerate destinations it can reach."
        >
          {scanning() ? "🔍 Scanning…" : "🔍 Scan"}
        </button>
        <OnboardButton
          connector={props.connector}
          formInputs={props.formInputs}
          onDiscovered={(info, matched) => {
            setLiveDiscoveries((prev) => ({
              ...prev,
              [info.workspace_key]: info,
            }));
            if (matched) {
              props.onChange(info.workspace_key);
            }
          }}
        />
        <button
          type="button"
          class="colony-text-3xs rounded border border-bark bg-soil-mid px-2 py-1 hover:bg-soil-light"
          onClick={() => setManualOpen((v) => !v)}
        >
          {manualOpen() ? "Close manual entry" : "✏️ Manual entry"}
        </button>
      </div>

      <Show when={manualOpen()}>
        <div class="mt-2 flex gap-2">
          <input
            type="text"
            class="colony-text-xs flex-1 border-2 border-bark bg-soil-deep px-3 py-2 text-text-primary focus:border-accent focus:outline-none"
            placeholder={placeholderForConnector(props.connector)}
            value={manualEntry()}
            onInput={(e) => setManualEntry(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onCommitManual();
            }}
          />
          <button
            type="button"
            class="colony-text-3xs rounded border border-bark bg-soil-deep px-3 py-2 text-text-primary hover:bg-soil-light"
            onClick={onCommitManual}
            disabled={manualEntry().trim().length === 0}
          >
            Add
          </button>
        </div>
        <p class="colony-text-3xs mt-1 text-text-dim">
          Type a raw id ({placeholderForConnector(props.connector)}) or a
          full URI like <code>{schemeForConnector(props.connector)}://…</code>.
        </p>
      </Show>

      <Show when={error()}>
        <p class="colony-text-3xs mt-1 text-status-warn">{error()}</p>
      </Show>
    </div>
  );
};

/**
 * Connector-agnostic Onboard button.
 *
 * On click, hands the deploy form's `formInputs` map (which contains
 * the user-supplied credentials — `bot_token`, `app_token`, etc.) to
 * the runtime's connector factory, which instantiates a one-shot
 * connector and dispatches its `onboard_url` action. The returned
 * URL is copied to the clipboard verbatim. Telegram's `onboard_url`
 * yields `https://t.me/<bot>?start=…`; other connectors that ship
 * an `onboard_url` action plug into the same path. Connectors
 * without one surface a clear error.
 */
/** Stable session token for one Onboard click. Embedded in the
 *  `t.me/<bot>?start=<payload>` deep link so the backend stream's
 *  `discover_destinations` payload-filter can distinguish the user's
 *  own /start from any unrelated traffic the bot received. Short,
 *  URL-safe, base64url over crypto.getRandomValues. Telegram caps
 *  the payload at 64 chars. */
function mintOnboardPayload(): string {
  const bytes = new Uint8Array(12);
  crypto.getRandomValues(bytes);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  // base64url, no padding
  return `springtale-onboard-${btoa(bin)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "")}`;
}

const OnboardButton: Component<{
  connector: string;
  formInputs?: Record<string, unknown>;
  /** Fired once for each chat the backend's auto-poll discovers.
   *  `matched=true` means it passed the `/start <payload>` filter
   *  — the picker auto-selects this chat. */
  onDiscovered: (info: WorkspaceInfo, matched: boolean) => void;
}> = (props) => {
  const db = useDashboard();
  const [copied, setCopied] = createSignal(false);
  const [pending, setPending] = createSignal(false);
  const [streaming, setStreaming] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);

  // Track D — active stream lifecycle. The session id is regenerated
  // on every Onboard click (along with the payload nonce) so a fresh
  // tap doesn't merge results with the previous attempt.
  let activeSession: string | null = null;
  let activeUnlisten: (() => void) | null = null;

  const stopActiveSession = () => {
    if (activeUnlisten) {
      activeUnlisten();
      activeUnlisten = null;
    }
    if (activeSession) {
      const sid = activeSession;
      activeSession = null;
      void db.provider.cancelOnboardStream(sid).catch(() => {});
    }
    setStreaming(false);
  };

  // Always tear down the stream when the picker unmounts so the
  // tokio task on the backend doesn't outlive the UI by 60s.
  onCleanup(stopActiveSession);

  // WebKit bug 222262 — `navigator.clipboard.writeText` after an `await`
  // is rejected by WKWebView (Tauri's macOS engine) because user
  // activation expires across the await boundary. The published WebKit
  // workaround is to pass a `Promise<Blob>` into `ClipboardItem` and
  // call `navigator.clipboard.write([...])` SYNCHRONOUSLY in the click
  // handler. WebKit / Chromium / Firefox all accept that shape — one
  // path, every platform, no Tauri plugin.
  // https://bugs.webkit.org/show_bug.cgi?id=222262
  // https://webkit.org/blog/10855/async-clipboard-api/
  const onClick = () => {
    setFailure(null);
    setPending(true);
    setCopied(false);

    // Tear down any prior session (user re-clicked Onboard).
    stopActiveSession();

    const sessionId = crypto.randomUUID();
    const payload = mintOnboardPayload();
    activeSession = sessionId;

    const config = (props.formInputs ?? {}) as Record<string, unknown>;

    // Kick the auto-poll stream first so the listener subscribes
    // before the backend has any chance to emit. Subscribe is a
    // Promise but the listener is registered the moment the resolve
    // settles — which is faster than the user can possibly tap START.
    db.provider
      .subscribeToChatDiscovered((event: ChatDiscoveredEvent) => {
        if (event.session_id !== sessionId) return;
        const info: WorkspaceInfo = {
          workspace_key: event.workspace_key,
          connector_name: props.connector,
          display_name: event.display_name,
          kind: event.kind,
          metadata_json: event.metadata_json,
          first_seen_at_unix_ms: Date.now(),
          last_seen_at_unix_ms: Date.now(),
          provenance_json: "",
        };
        props.onDiscovered(info, event.matched);
        if (event.matched) {
          stopActiveSession();
        }
      })
      .then((unlisten) => {
        // If the user already cancelled while the listener was attaching,
        // immediately tear it back down.
        if (activeSession !== sessionId) {
          unlisten();
          return;
        }
        activeUnlisten = unlisten;
        // Listener is live — now start the backend stream.
        return db.provider
          .startOnboardStream(sessionId, props.connector, config, payload)
          .then(() => setStreaming(true));
      })
      .catch((e: unknown) => {
        setFailure(e instanceof Error ? e.message : String(e));
        stopActiveSession();
      });

    // Clipboard write fires SYNCHRONOUSLY — the URL fetch is wrapped
    // in a Promise-backed ClipboardItem so user activation stays alive.
    const urlPromise = db.provider.previewOnboardUrl(
      props.connector,
      config,
      payload,
    );
    const blobPromise: Promise<Blob> = urlPromise.then((url) => {
      if (!url) {
        throw new Error(
          "Onboard URL not available — fill in your connector credentials above first.",
        );
      }
      return new Blob([url], { type: "text/plain" });
    });

    navigator.clipboard
      .write([new ClipboardItem({ "text/plain": blobPromise })])
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2500);
      })
      .catch((e: unknown) => {
        setFailure(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        setPending(false);
      });
  };

  return (
    <div class="flex flex-col gap-1">
      <button
        type="button"
        class="colony-text-3xs rounded border border-bark bg-soil-mid px-2 py-1 hover:bg-soil-light disabled:opacity-50"
        onClick={onClick}
        disabled={pending()}
        title="Resolve the connector's onboarding URL, copy it to your clipboard, and watch for the chat to register."
      >
        {pending()
          ? "⌛ Resolving…"
          : copied()
            ? streaming()
              ? "✓ Link copied · watching…"
              : "✓ Link copied"
            : "🎯 Onboard"}
      </button>
      <Show when={streaming() && !failure()}>
        <p class="colony-text-3xs text-text-dim">
          Tap the link in Telegram and press START — the chat will appear here.
        </p>
      </Show>
      <Show when={failure()}>
        <p class="colony-text-3xs text-status-warn">{failure()}</p>
      </Show>
    </div>
  );
};

function placeholderForConnector(connector: string): string {
  switch (connector) {
    case "connector-telegram":
      return "12345 or @channelusername";
    case "connector-discord":
      return "1234567890123456 (channel id)";
    case "connector-slack":
      return "C12345 (channel id)";
    case "connector-signal":
      return "+15551234 or group-id";
    case "connector-irc":
      return "#channel or nickname";
    case "connector-nostr":
      return "pubkey hex";
    case "connector-bluesky":
      return "did:plc:…";
    default:
      return "destination id";
  }
}

function schemeForConnector(connector: string): string {
  return connector.startsWith("connector-")
    ? connector.slice("connector-".length)
    : connector;
}

function guessKindFromKey(key: string): string {
  // Best-effort default. The backend's MentionExtractor maps types
  // properly; manual entries get a coarse classification so the
  // dropdown's filter behavior still works.
  if (key.startsWith("#") || key.startsWith("&")) return "channel";
  if (key.startsWith("@")) return "channel";
  if (key.startsWith("did:")) return "account";
  if (/^-?\d+$/.test(key)) return "user";
  if (key.includes("://")) {
    // URI form — derive kind from second segment.
    try {
      const after = key.split("://", 2)[1] ?? "";
      const seg = after.split("/")[0];
      if (seg === "chat") return "user";
      if (seg === "channel") return "channel";
      if (seg === "guild") return "channel";
      if (seg === "dm" || seg === "im") return "dm";
      if (seg === "group") return "group";
      if (seg === "account") return "account";
      if (seg === "pubkey") return "user";
      if (seg === "user") return "user";
    } catch {
      /* fall through */
    }
  }
  return "user";
}

function relativeTime(unixMs: number): string {
  if (!Number.isFinite(unixMs) || unixMs <= 0) return "—";
  const deltaMs = Date.now() - unixMs;
  const sec = Math.max(0, Math.floor(deltaMs / 1000));
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  return `${day}d ago`;
}
