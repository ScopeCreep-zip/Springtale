# API client: JavaScript / TypeScript

Driving the `springtaled` HTTP API from Node or the browser. Uses
the platform `fetch` so it works in both.

The Tauri desktop frontend talks to its **own** in-process Tauri
command set, not this API. This doc is for **web** clients of the
daemon: dashboards, monitoring panels, third-party integrations.

## Install (browser via `<script>`)

```html
<script type="module">
  import { Springtale } from "https://cdn.jsdelivr.net/npm/@scopecreep/springtale-client@latest/index.mjs";
</script>
```

(Not yet published. Roadmap.)

## Install (Node / bundler)

```bash
npm install @scopecreep/springtale-client
```

(Also not yet published. Until then, drop the file below into your
project — it's small.)

## Minimal client

```typescript
type FormationView = {
  id: string;
  intent: { kind: string; payload?: string };
  momentum_tier: "Cold" | "Warming" | "Hot" | "Fever";
  rally_tokens: number;
  member_count: number;
};

export class Springtale {
  constructor(
    private host: string = "http://127.0.0.1:8080",
    private token: string = "",
  ) {
    this.host = host.replace(/\/$/, "");
  }

  private async req<T>(
    path: string,
    init: RequestInit = {},
  ): Promise<T> {
    const r = await fetch(`${this.host}${path}`, {
      ...init,
      headers: {
        Authorization: `Bearer ${this.token}`,
        "Content-Type": "application/json",
        ...(init.headers || {}),
      },
    });
    if (!r.ok) {
      const body = await r.json().catch(() => ({}));
      throw new SpringtaleError(r.status, body);
    }
    return r.json() as Promise<T>;
  }

  health() { return this.req<{ status: string; version: string }>("/health"); }
  listConnectors() { return this.req<unknown[]>("/connectors"); }
  listRules() { return this.req<unknown[]>("/rules"); }
  listFormations() { return this.req<FormationView[]>("/formations"); }

  reloadConnector(name: string) {
    return this.req<{ reloaded: string }>(
      `/connectors/${encodeURIComponent(name)}/reload`,
      { method: "POST" },
    );
  }

  setDisguiseActive(active: boolean) {
    return this.req<{ saved: boolean }>(
      "/safety/disguise/active",
      { method: "POST", body: JSON.stringify({ active }) },
    );
  }
}

export class SpringtaleError extends Error {
  constructor(public status: number, public body: any) {
    super(`Springtale API error ${status}: ${body?.error?.message ?? "unknown"}`);
  }
}
```

## TypeScript types from the daemon

The daemon's IPC layer emits TypeScript types via `ts-rs` into
`tauri/packages/types/src/generated/`:

- `FormationDelta.ts`
- `FormationOutcome.ts`
- `FormationStatus.ts`
- `FormationView.ts`
- `index.ts` (re-exports)

These types are generated from the Rust source — they're guaranteed
to match the wire shape. Import them in your TS project:

```bash
npm install @springtale/types     # workspace package
```

```typescript
import type { FormationView, FormationDelta } from "@springtale/types";
```

The package isn't yet on npm; for now, vendor the files into your
project or symlink them.

## SSE streams

Browser:

```typescript
const source = new EventSource(
  `${host}/events/stream?token=${encodeURIComponent(token)}`,
);

source.addEventListener("connector_event", (e) => {
  const payload = JSON.parse(e.data);
  console.log("connector event:", payload);
});

source.addEventListener("rule_fired", (e) => {
  const payload = JSON.parse(e.data);
  console.log("rule fired:", payload);
});

source.onerror = (e) => {
  console.error("SSE error:", e);
  source.close();
};
```

Node (with `eventsource` polyfill):

```typescript
import EventSource from "eventsource";

const source = new EventSource(
  `${host}/events/stream?token=${encodeURIComponent(token)}`,
);
// ...same handlers as browser...
```

## Watching cooperation events

```typescript
const source = new EventSource(
  `${host}/cooperation/events?token=${encodeURIComponent(token)}&formation_id=research-squad`,
);

source.addEventListener("momentum_transition", (e) => {
  const { from, to, formation_id } = JSON.parse(e.data);
  updateDashboardBar(formation_id, from, to);
});

source.addEventListener("rally_burned", (e) => {
  const { formation_id, tokens_remaining } = JSON.parse(e.data);
  updateRallyPips(formation_id, tokens_remaining);
});
```

## In-app chat

```typescript
// Send a message — fire-and-forget, the reply arrives on the stream:
await fetch(`${host}/chat`, {
  method: "POST",
  headers: {
    Authorization: `Bearer ${token}`,
    "Content-Type": "application/json",
  },
  body: JSON.stringify({ text: "what's running right now?" }),
});

// Stream the bot's replies (default `message` events, no custom name):
const chat = new EventSource(
  `${host}/chat/stream?token=${encodeURIComponent(token)}`,
);
chat.onmessage = (e) => {
  const { session, text } = JSON.parse(e.data);
  appendChatBubble(session, text);
};
```

## Approvals queue

```typescript
async function listPendingApprovals() {
  const res = await fetch(`${host}/approvals`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  const { pending } = await res.json();
  return pending;
}

async function resolveApproval(id: string, approve: boolean, reason?: string) {
  const res = await fetch(`${host}/approvals/${id}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      decision: approve ? "approve" : "deny",
      ...(reason ? { reason } : {}),
    }),
  });
  // 404 = timed out already, 409 = someone else resolved it first.
  return res.ok;
}
```

## Auth in the browser

The bearer token can be:

- **A user-provided value** typed into a settings page.
- **Read from a same-origin endpoint** that proxies the daemon and
  attaches the token server-side. Recommended for production
  dashboards.
- **From the daemon itself** if the dashboard is served by
  `springtaled` (the bundled SPA). The daemon serves the SPA + a
  cookie with the token at the same origin.

Don't bake the token into your JS bundle if your bundle is publicly
served.

## Reading dashboard config

If you're building a custom dashboard against a daemon, the dashboard
config (theme, layout, last-selected-formation) lives in the daemon's
runtime_config table:

```typescript
const config = await sp.req<{ theme: string; layout: string }>(
  "/config/dashboard",
);
```

## Error handling

```typescript
try {
  await sp.reloadConnector("connector-doesnt-exist");
} catch (e) {
  if (e instanceof SpringtaleError) {
    if (e.status === 404) {
      console.warn("connector not installed");
    } else if (e.status === 429) {
      console.warn("rate limited; backoff");
    } else {
      console.error(`Springtale error: ${e.body?.error?.code}: ${e.body?.error?.message}`);
    }
  } else {
    throw e;
  }
}
```

## Webhook signature verification (server-side)

If you're building a service that receives webhooks **from** Springtale:

```typescript
import { createHmac, timingSafeEqual } from "node:crypto";

function verifyWebhook(
  payload: Buffer,
  signatureHeader: string,
  secret: string,
): boolean {
  const expected = "sha256=" + createHmac("sha256", secret).update(payload).digest("hex");
  return timingSafeEqual(
    Buffer.from(expected, "utf8"),
    Buffer.from(signatureHeader, "utf8"),
  );
}
```

## Gotchas

- **CORS.** The daemon doesn't currently allow cross-origin requests
  from arbitrary domains. For a dashboard hosted separately from the
  daemon, run a same-origin proxy or set `[api] cors_origins` in the
  daemon config.
- **EventSource doesn't support custom headers.** That's why SSE
  endpoints accept the token via query string. The query-string
  variant is still logged in the audit trail.
- **EventSource reconnects with no backoff.** A daemon restart will
  cause hammer-reconnects from every dashboard. The Tauri desktop
  has its own reconnect with backoff; browser clients should add
  that manually.
- **JSON.stringify a Date** produces ISO-8601 with milliseconds.
  Server expects exactly that. Don't pass Date objects directly; the
  default coercion is what you want.
- **Bigints.** Some cooperation counters are u64. JSON numbers are
  doubles, so values >2^53 lose precision. The wire format uses
  strings for u64s above that threshold; check `typeof` before
  parsing.
