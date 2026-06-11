use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use specta::Type;

use springtale_crypto::signature::SignatureAlgorithm;

/// A connector's manifest — the declaration of what it is, what it needs,
/// and what it can do. Parsed from `connector-{name}.toml`.
///
/// Every connector ships with a manifest. For native connectors, the manifest
/// is embedded. For WASM connectors, it accompanies the `.wasm` binary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Type)]
#[serde(deny_unknown_fields)]
pub struct ConnectorManifest {
    /// Connector name (e.g., "connector-kick").
    pub name: String,

    /// Semantic version (e.g., "1.0.0").
    pub version: String,

    /// Author name or organization.
    pub author: String,

    /// Human-readable description.
    pub description: String,

    /// Required capabilities — what this connector needs to function.
    pub capabilities: Vec<Capability>,

    /// Events this connector can emit.
    #[serde(default)]
    pub triggers: Vec<TriggerDecl>,

    /// Actions this connector can perform.
    #[serde(default)]
    pub actions: Vec<ActionDecl>,

    /// What user data this connector accesses (transparency).
    #[serde(default)]
    pub data_disclosure: Vec<DataDisclosure>,

    /// Custom cooperation roles this connector contributes to the
    /// shared `RoleRegistry` (§14.4). Each role is identified by name
    /// and carries an action-glob allowlist; they're exposed alongside
    /// the built-in General/Information/Support roles once the
    /// connector is installed.
    #[serde(default)]
    pub roles: Vec<RoleDecl>,

    /// SHA-256 hash of the WASM binary (for WASM connectors only).
    #[serde(default)]
    pub wasm_hash: Option<String>,

    /// Algorithm used to compute the `signature` field. Defaults to
    /// `SignatureAlgorithm::Ed25519` to keep older manifests parsing.
    /// Per NIST IR 8547 the Ed25519 path is deprecated 2030 /
    /// disallowed 2035; the enum is the extension point for the
    /// hybrid Ed25519+ML-DSA-65 successor (see
    /// `docs/security/CRYPTO-INVENTORY.md`). Unknown algorithm names
    /// fail closed at parse time.
    #[serde(default)]
    pub signature_alg: SignatureAlgorithm,

    /// Signature over the canonical JSON of all other fields (the
    /// `signature` field itself is excluded). Encoding depends on
    /// `signature_alg`: for `Ed25519`, hex-encoded 64-byte Ed25519
    /// signature.
    #[serde(default)]
    pub signature: Option<String>,
}

/// A capability that a connector requires.
///
/// Capabilities are declared in the manifest and verified at install time.
/// The runtime checks capabilities BEFORE every `execute()` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Type)]
#[serde(tag = "type")]
pub enum Capability {
    /// Outbound network access to a specific host.
    /// No wildcards — exact host match only.
    NetworkOutbound { host: String },

    /// Read access to a filesystem path.
    FilesystemRead { path: String },

    /// Write access to a filesystem path.
    FilesystemWrite { path: String },

    /// Read a specific key from the keychain/vault.
    KeychainRead { key: String },

    /// Execute shell commands. Triggers a blocking approval prompt.
    ShellExec,
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Capability::NetworkOutbound { host } => write!(f, "NetworkOutbound:{host}"),
            Capability::FilesystemRead { path } => write!(f, "FilesystemRead:{path}"),
            Capability::FilesystemWrite { path } => write!(f, "FilesystemWrite:{path}"),
            Capability::KeychainRead { key } => write!(f, "KeychainRead:{key}"),
            Capability::ShellExec => write!(f, "ShellExec"),
        }
    }
}

/// Declaration of an event a connector can emit.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Type)]
pub struct TriggerDecl {
    /// Trigger name (e.g., "stream_live", "chat_message").
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// JSON Schema of the event payload.
    #[serde(default)]
    pub schema: Option<serde_json::Value>,
}

/// Declaration of an action a connector can perform.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Type)]
pub struct ActionDecl {
    /// Action name (e.g., "send_chat", "create_post").
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// JSON Schema of the action input parameters.
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,

    /// JSON Schema of the action output.
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,

    /// Whether this action is read-only — it retrieves data and never
    /// creates, updates, deletes, or sends anything. Modelled on the MCP
    /// tool annotation `readOnlyHint`: a conservative **advisory** hint
    /// (default `false` = assume the action may mutate), *not* a security
    /// boundary. The deterministic safety gate stays in `springtale-sentinel`
    /// / the capability layer. Consumed by the formation intent decomposer to
    /// pick read-only actions under a `Reconnoiter` (monitor) intent.
    ///
    /// Strict, per MCP guidance: only `true` when the action genuinely cannot
    /// modify state — a "search" that also logs analytics is *not* read-only.
    #[serde(default)]
    pub read_only: bool,
}

/// What user data a connector accesses (for transparency/Privacy by Design).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Type)]
pub struct DataDisclosure {
    /// What data is accessed (e.g., "chat messages", "stream metadata").
    pub data_type: String,

    /// Why this data is needed.
    pub purpose: String,

    /// Where the data is sent (e.g., "api.kick.com", "local only").
    pub destination: String,
}

/// A custom cooperation role contributed by a connector.
///
/// Registered into `springtale_cooperation::role::RoleRegistry` at
/// install time. See the registry module for semantics; see
/// `CommunityRole` for how `allowed_actions` patterns are matched.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Type)]
#[serde(deny_unknown_fields)]
pub struct RoleDecl {
    /// Human-readable role name. Namespaced by convention (e.g.,
    /// `"github/read-only-auditor"`) to avoid clobbering the built-in
    /// `General`/`Information`/`Support` roles.
    pub name: String,

    /// Short description for the role picker UI.
    #[serde(default)]
    pub description: String,

    /// Capabilities this role exposes. When empty, the role inherits
    /// whatever capabilities the member already holds — useful for
    /// filter-only roles like "Watcher" that don't change the cap set.
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Action-name allowlist. Trailing `*` is a prefix wildcard; no
    /// other patterns are supported. An empty list denies everything
    /// (the role can hold capabilities but never dispatch actions —
    /// rarely useful outside quarantine scenarios).
    #[serde(default)]
    pub allowed_actions: Vec<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_display() {
        assert_eq!(
            Capability::NetworkOutbound {
                host: "api.kick.com".into()
            }
            .to_string(),
            "NetworkOutbound:api.kick.com"
        );
        assert_eq!(Capability::ShellExec.to_string(), "ShellExec");
    }

    #[test]
    fn test_manifest_deserialize_from_toml() {
        let toml_str = r#"
            name = "connector-test"
            version = "1.0.0"
            author = "test"
            description = "A test connector"

            [[capabilities]]
            type = "NetworkOutbound"
            host = "api.example.com"

            [[triggers]]
            name = "event_happened"
            description = "Fires when something happens"

            [[actions]]
            name = "do_thing"
            description = "Does a thing"

            [[actions]]
            name = "read_thing"
            description = "Reads a thing"
            read_only = true

            [[data_disclosure]]
            data_type = "user messages"
            purpose = "formatting responses"
            destination = "local only"
        "#;

        let manifest: ConnectorManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.name, "connector-test");
        assert_eq!(manifest.capabilities.len(), 1);
        assert_eq!(
            manifest.capabilities[0],
            Capability::NetworkOutbound {
                host: "api.example.com".into()
            }
        );
        assert_eq!(manifest.triggers.len(), 1);
        assert_eq!(manifest.actions.len(), 2);
        assert_eq!(manifest.data_disclosure.len(), 1);

        // `read_only` defaults to false (MCP `readOnlyHint` conservative
        // default) when the manifest omits it, and parses when present.
        assert!(!manifest.actions[0].read_only, "do_thing defaults to false");
        assert!(manifest.actions[1].read_only, "read_thing is read-only");
    }
}
