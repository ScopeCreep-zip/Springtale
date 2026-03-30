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

## Connector Trait Implementation

Every connector implements `springtale_connector::Connector`:
- `triggers()` — what events this connector emits
- `actions()` — what actions it can perform
- `execute(action, input)` — execute an action (capability-checked by runtime)
- `on_event(trigger, handler)` — register event handler
- `manifest()` — return connector metadata

## Manifest

Every connector ships with a `connector-{name}.toml` manifest declaring:
- Name, version, author, description
- Required capabilities (NetworkOutbound hosts, FilesystemRead paths, etc.)
- Trigger declarations with typed schemas
- Action declarations with typed input/output schemas
- DataDisclosure: what user data the connector accesses
