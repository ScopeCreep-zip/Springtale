# Adding a Connector

Step-by-step guide to building a new first-party connector for Springtale.

## 1. Decide: Native or WASM

| | Native | WASM |
|---|---|---|
| **When** | First-party, audited by the team | Community-authored, untrusted |
| **Language** | Rust | TypeScript SDK → wasm32-wasip2, or Rust |
| **Isolation** | In-process, capability-checked | Wasmtime sandbox (10M fuel, 64MB mem, 30s timeout) |
| **Trust** | High | Low |

This guide covers native Rust connectors. WASM connector authoring will be covered once the TypeScript SDK lands.

---

## 2. Scaffold the Crate

```
connectors/connector-{name}/
├── Cargo.toml
└── src/
    ├── lib.rs              # pub mod + re-exports ONLY
    ├── config.rs           # Config struct (Deserialize only, Secret<String> for credentials)
    ├── connector.rs        # Connector trait implementation
    ├── factory.rs          # (optional) ConnectorFactory impl — needed if the connector
    │                       #   is constructed from a typed config struct rather than a
    │                       #   bare config map. Most chat connectors have one; see
    │                       #   connector-discord, connector-kick, connector-telegram.
    ├── error.rs            # Typed error enum (thiserror)
    ├── auth/               # (optional) Auth flow modules — present when the flow is
    │   └── mod.rs          #   complex enough to warrant its own module (OAuth 2.1 PKCE,
    │                       #   SASL, etc.). For simple bearer-token auth, inline it
    │                       #   into client/. See connector-kick for a full OAuth flow.
    ├── client/             # Typed HTTP client (all network calls here)
    │   ├── mod.rs
    │   └── api.rs
    ├── triggers/           # One module per trigger type
    │   └── mod.rs
    └── actions/            # One module per action type
        └── mod.rs
```

*Fig. 1. Standard connector directory layout. `factory.rs` and `auth/` are optional but common — include them when their concern is non-trivial.*

### 2.1. Cargo.toml

```toml
[package]
name = "connector-{name}"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
springtale-connector = { workspace = true }
springtale-crypto = { workspace = true }
reqwest = { workspace = true }
secrecy = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
```

All version pins come from the workspace root `Cargo.toml`. Never specify versions in connector crates.

---

## 3. Implement the Trait

The `Connector` trait lives in `springtale-connector`:

```rust
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    fn triggers(&self) -> &[TriggerDecl];
    fn actions(&self) -> &[ActionDecl];

    async fn execute(&self, action: &str, input: serde_json::Value)
        -> Result<ActionResult, ConnectorError>;

    /// Register a trigger handler. Returns a Subscription handle that
    /// the caller stores per-rule and passes to `remove_event()` when
    /// the rule is disabled, deleted, or updated (Home Assistant
    /// attach_trigger → detach pattern).
    async fn on_event(&self, trigger: &str, handler: EventHandler)
        -> Result<Subscription, ConnectorError>;

    /// Remove a previously registered handler by its subscription.
    async fn remove_event(&self, sub: &Subscription)
        -> Result<(), ConnectorError>;

    fn manifest(&self) -> &ConnectorManifest;

    async fn verify_webhook(&self, headers: &HeaderMap, body: &[u8])
        -> Result<(), ConnectorError>;  // default: reject
}
```

Implement this in `connector.rs`. The struct should hold the config, an HTTP client, and any runtime state (auth tokens, caches, a `SubscriptionCounter`, a handler list keyed by `SubscriptionId`).

### 3.1. Subscription lifecycle

`on_event()` must return a `Subscription` that uniquely identifies the
handler. Use `SubscriptionCounter` from `springtale-connector` to mint
ids. Store handlers in an internal `HashMap<SubscriptionId,
EventHandler>` (or a per-trigger `Vec<(SubscriptionId, EventHandler)>`).

`remove_event()` removes the handler matching the subscription's id.
The bot calls `remove_event()` from its rule lifecycle — when a rule is
disabled or deleted, every `Subscription` it owns is torn down in one
sweep. Connectors that forget to implement `remove_event()` will leak
handlers; the capability layer will not catch that.

The trait is shared by native and WASM connectors. WASM host functions
marshal `SubscriptionId` across the sandbox boundary so subscription
bookkeeping works uniformly on both execution paths.

---

## 4. Config with Secrets

Config structs derive `Deserialize` only — never `Serialize` (this prevents secrets from appearing in serialized output).

```rust
use secrecy::SecretBox;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct MyConfig {
    pub api_key: SecretBox<String>,  // Secret<String> — cannot be logged or serialized
    pub api_base: String,            // Non-secret fields are plain types
    pub timeout_secs: u64,
}
```

Every credential, token, and password is `Secret<String>`. Every `.expose_secret()` call is annotated with `// SECURITY: expose needed for X`.

---

## 5. Write the Client

All network calls go through a typed client in `client/`. No raw `reqwest` calls outside this module.

```rust
pub struct MyClient {
    http: reqwest::Client,
    api_base: String,
    api_key: SecretBox<String>,
}

impl MyClient {
    pub async fn search(&self, query: &str) -> Result<SearchResult, MyError> {
        let resp = self.http
            .get(format!("{}/search", self.api_base))
            .header("Authorization", format!(
                "Bearer {}",
                self.api_key.expose_secret() // SECURITY: expose for API auth header
            ))
            .query(&[("q", query)])
            .send()
            .await?;
        // ...
    }
}
```

The `reqwest` client must use `rustls-tls` (this is enforced at the workspace level — `native-tls` is banned).

---

## 6. Add Triggers

Triggers declare what events your connector emits. Each trigger has a name and a typed payload schema.

For webhook-based triggers, implement signature verification in your connector (HMAC-SHA256, RSA, etc.). The daemon's `/webhook/{connector}/{trigger}` endpoint forwards the raw request body to your handler.

For polling/streaming triggers (like Bluesky's Jetstream), implement the subscription loop and call the `EventHandler` when events arrive.

---

## 7. Add Actions

Actions declare what your connector can do. Each action has a name, typed input, and typed output.

Declare the capabilities your actions require in the manifest. The runtime checks capabilities before every `execute()` call — your action will never run without the required permissions.

---

## 8. Write the Manifest

Every connector ships with a TOML manifest:

```toml
[connector]
name = "connector-myservice"
version = "0.1.0"
author = "Your Name"
description = "MyService integration for Springtale"

[[capabilities]]
type = "NetworkOutbound"
host = "api.myservice.com"

[[triggers]]
name = "item_created"
description = "Fires when a new item is created"

[[actions]]
name = "create_item"
description = "Create a new item"

[data_disclosure]
description = "Sends item title and body to api.myservice.com"
```

---

## 9. Test

### 9.1. Unit Tests

Place in the same file as the implementation:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_validates_input() {
        // ...
    }

    #[tokio::test]
    async fn test_execute_returns_result() {
        // ...
    }
}
```

### 9.2. Mock at the Client Layer

Don't mock `reqwest` directly. Create a trait for your client and provide a mock implementation for tests. This keeps tests fast and deterministic.

### 9.3. Test Naming

Pattern: `test_{function}_{scenario}_{expected}`

```rust
#[test]
fn test_search_empty_query_returns_error() { ... }

#[test]
fn test_search_valid_query_returns_results() { ... }
```

---

## 10. Sign and Register

Manifests can be signed with Ed25519 for verification:

```bash
# Generate a signing key (or use the vault keypair)
# Sign the manifest
# The signature is verified at install time and on every load
```

Add the connector to the workspace `Cargo.toml`:

```toml
[workspace]
members = [
    # ...existing members...
    "connectors/connector-myservice",
]
```

---

## References

- [1] Connector trait: `crates/springtale-connector/src/connector/trait_.rs`
- [2] Capability types: `crates/springtale-connector/src/manifest/types.rs`
- [3] Connector guidelines: `.claude/rules/connector-guidelines.md`
- [4] Testing conventions: `.claude/rules/testing.md`
- [5] Security rules: `.claude/rules/security.md`
- [6] Existing connectors for reference: `connectors/connector-kick/`, `connectors/connector-github/`
