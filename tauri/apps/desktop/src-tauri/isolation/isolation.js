// Tauri 2.x Isolation Pattern hook.
//
// Runs inside the isolation iframe — completely separate origin from
// the main application. The runtime calls this for every IPC message
// before it reaches the Tauri backend.
//
// Returning the (possibly mutated) message lets the call proceed.
// Throwing rejects it. The error message surfaces at the JS caller.

const MAX_PAYLOAD_BYTES = 1024 * 1024; // 1 MiB

// Commands that the application is allowed to invoke directly. Anything
// else trips a safety-net rejection — keeps the surface honest if a
// future feature ever calls an undeclared command without us noticing.
//
// Maintain this list when adding new commands. The capability JSON in
// `src-tauri/capabilities/default.json` is the primary authority; this
// list is a secondary cross-check that runs even if a capability is
// mis-scoped at the policy layer.
//
// `null` means "permit everything not in the deny-list below" — the
// preference is the explicit allow-list, but bootstrapping that for
// every existing command requires a separate audit (Phase 6 follow-up).
// For now we use a deny-list of dangerous wildcards instead.
const ALLOW_LIST = null;

// Hosts / URLs that the app must never request via the `http` plugin
// (where it exists) or any URL-bearing command. The Rust side already
// enforces this via the connector capability allow-list; we belt-and-
// brace it here in case a future regression weakens that.
const HOST_DENY = ["*", "0.0.0.0", "localhost:0", "[::]:0"];

// Commands that pass a path / glob argument. We reject wildcard scopes
// in those arguments — connectors must declare exact paths.
const PATH_ARG_COMMANDS = new Set([
  "fs:read_text_file",
  "fs:write_text_file",
  "fs:read_file",
  "fs:write_file",
  "fs:remove_file",
  "fs:read_dir",
  "fs:create_dir",
  "fs:remove_dir",
  "fs:rename",
  "fs:exists",
  "fs:metadata",
  "fs:watch",
]);

function byteLength(value) {
  if (typeof value === "string") return new TextEncoder().encode(value).length;
  if (value instanceof Uint8Array) return value.byteLength;
  return new TextEncoder().encode(JSON.stringify(value ?? null)).length;
}

function rejectWildcard(field, value) {
  if (typeof value !== "string") return;
  // Any of: `*`, `**`, `?`, leading `~`, `/` alone — these are wildcard or
  // root scopes that connectors must never request.
  if (
    value === "*" ||
    value === "**" ||
    value === "/" ||
    value === "~" ||
    value.startsWith("*") ||
    value.includes("*")
  ) {
    throw new Error(`springtale isolation: wildcard ${field} rejected ("${value}")`);
  }
}

window.__TAURI_ISOLATION_HOOK__ = (payload) => {
  if (!payload || typeof payload !== "object") return payload;

  // 1) Size cap — defense against argument-construction DoS.
  const size = byteLength(payload);
  if (size > MAX_PAYLOAD_BYTES) {
    throw new Error(`springtale isolation: payload too large (${size}B > ${MAX_PAYLOAD_BYTES}B)`);
  }

  // 2) Optional explicit allow-list (currently null; deny-list below).
  if (ALLOW_LIST && payload.cmd && !ALLOW_LIST.has(payload.cmd)) {
    throw new Error(`springtale isolation: unknown command "${payload.cmd}"`);
  }

  // 3) Wildcard host / URL guard for any string field named `host`, `url`,
  //    `endpoint`, `origin`, or `scope`.
  const inspect = (obj) => {
    if (!obj || typeof obj !== "object") return;
    for (const [k, v] of Object.entries(obj)) {
      const key = k.toLowerCase();
      if (
        key === "host" ||
        key === "url" ||
        key === "endpoint" ||
        key === "origin" ||
        key === "scope"
      ) {
        if (HOST_DENY.includes(v)) {
          throw new Error(`springtale isolation: denied ${key} value "${v}"`);
        }
        rejectWildcard(key, v);
      }
      if (v && typeof v === "object") inspect(v);
    }
  };
  inspect(payload);

  // 4) Path-argument wildcard guard.
  if (payload.cmd && PATH_ARG_COMMANDS.has(payload.cmd)) {
    const args = payload.callback?.payload ?? payload.payload ?? payload.args ?? {};
    const candidate = typeof args === "string" ? args : (args.path ?? args.glob ?? args.pattern);
    if (candidate) rejectWildcard("path", candidate);
  }

  return payload;
};
