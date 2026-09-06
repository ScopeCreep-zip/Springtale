---
paths:
  - "connectors/**/*.rs"
  - "connectors/**/Cargo.toml"
---

# Connector Development Guidelines

## Every first-party connector follows this structure:

```
connectors/connector-{name}/
└── src/
    ├── lib.rs              # pub mod + re-exports ONLY (no inline functions or types)
    ├── config.rs           # Config struct (Secret<String> for all credentials)
    ├── auth/               # Auth flows (OAuth2, API key, bearer token)
    │   ├── mod.rs
    │   └── ...
    ├── client/             # Typed API client (all network calls here)
    │   ├── mod.rs
    │   └── api.rs
    ├── triggers/           # One module per trigger type
    │   ├── mod.rs
    │   └── ...
    └── actions/            # One module per action type
        ├── mod.rs
        └── ...
```

## Connector Rules

1. Config structs derive `serde::Deserialize` ONLY — never `Serialize` (prevents secrets in logs).
2. ALL credentials as `Secret<String>`.
3. ALL network calls use `reqwest` with `rustls-tls`.
4. HMAC/signature verification on all incoming webhooks.
5. Typed error enums via `thiserror`. No `anyhow`.
6. Every action has a `#[cfg(test)]` module with mock client tests.
7. No raw `reqwest` calls outside the `client/` module.
8. Every `ActionDecl` sets `read_only` (MCP `readOnlyHint` semantics): `true` only
   when the action purely retrieves data and never creates/updates/deletes/sends
   (get/list/search/fetch). Default is `false` (assume mutating). It's an
   advisory hint the formation intent decomposer uses to pick actions under a
   `Reconnoiter` (monitor) intent — **not** a security boundary (that stays in
   `springtale-sentinel`). Be strict: a "search" that also logs analytics is
   *not* read-only.

## Connector Trait Implementation

Every connector implements `springtale_connector::Connector`:
- `triggers()` — what events this connector emits
- `actions()` — what actions it can perform
- `execute(action, input)` — execute an action (capability-checked by runtime)
- `on_event(trigger, handler)` — register event handler
- `manifest()` — return connector metadata

## Manifest

Every connector returns a `ConnectorManifest` from its `fn manifest()`.
It is **built in code, not parsed from a file** — there is no
`connector-{name}.toml`. Building it in code keeps the declaration compiled
and type-checked beside the `triggers()` and `actions()` it describes, so a
declaration cannot silently drift from the thing it declares.

`ConnectorManifest` (`crates/springtale-connector/src/manifest/types.rs`)
carries:

- Name, version, author, description
- `capabilities: Vec<Capability>` — `NetworkOutbound { host }` (exact host,
  no wildcards), `FilesystemRead`/`FilesystemWrite { path }`,
  `KeychainRead { key }`, `ShellExec` (blocking approval, never bypassable)
- `triggers: Vec<TriggerDecl>` — name, description, optional JSON Schema of
  the event payload
- `actions: Vec<ActionDecl>` — name, description, optional input/output JSON
  Schema, plus the three hints below
- `data_disclosure: Vec<DataDisclosure>` — what user data is touched, why,
  and where it is sent
- `roles: Vec<RoleDecl>` — custom cooperation roles contributed to the shared
  `RoleRegistry` at install time
- `wasm_hash` — SHA-256 of the `.wasm` binary, WASM connectors only
- `signature_alg` + `signature` — signature over the canonical JSON of every
  other field, verified before load. The manifest is serialized for signing
  and to travel with a WASM binary; that is the only reason it derives
  `Serialize`/`Deserialize`, not because anyone authors it as config.

### `ActionDecl` hints

All three carry MCP tool-annotation semantics and are **advisory only**. The
deterministic security boundary stays in `springtale-sentinel` and the
capability layer; never treat a hint as a gate.

- `read_only: bool` — MCP `readOnlyHint`. See rule 8 above. Default `false`
  (assume the action mutates).
- `destructive: Option<bool>` — MCP `destructiveHint`: whether the action
  performs a *destructive* update rather than an additive one. `None` means
  unknown and classifies as destructive, matching MCP's default of `true`.
  Meaningful only when `read_only == false`. Set it explicitly to `Some(false)`
  for a plain additive write (posting a new message) rather than leaving it
  `None` and having it treated as a delete.
- `poll_interval_secs: Option<u64>` — opt-in sensing cadence, in seconds.
  `None` (the default) means a formation never polls the action: work comes
  from the environment — triggers, handoffs, CFP awards, surfaces — not from
  a central poller. `Some(n)` permits polling on that interval, floored to
  5 s (Home Assistant's `update_interval` floor), and only if the action is
  also `read_only` and requires no parameters. Set it only for an action that
  is genuinely cheap and genuinely a sensor.
