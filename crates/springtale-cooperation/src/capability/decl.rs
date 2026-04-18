//! Capability declaration — the typed unit a connector advertises and
//! the formation checks at dispatch time.
//!
//! Per COOPERATION.md §16.1: replaces raw `String` capabilities everywhere.
//! Per Spring RTS UnitDef: named boolean flags (canAttack, canMove) checked
//! at command dispatch. Per FIPA service-description: { name, type, properties }.
//! Per Claude Agent SDK: tool definition { name, description, input_schema }.

use serde::{Deserialize, Serialize};

/// A typed capability declaration. Connectors advertise these in their
/// manifests; formations check them at task routing and interference
/// detection. One struct for all capability references — not separate
/// newtypes per capability kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CapabilityDecl {
    /// The capability name — matches connector action names.
    /// Examples: "github.read_issues", "slack.send_message", "read_env", "consensus".
    pub name: String,
    /// Which connector provides this capability. `None` for formation-level
    /// cooperation primitives (the momentum-unlocked set from binder/).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector: Option<String>,
    /// Arbitrary metadata — connector-specific schema, cost hints, rate
    /// limit info. Per Claude SDK: the input_schema field.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl CapabilityDecl {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connector: None,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn with_connector(name: impl Into<String>, connector: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connector: Some(connector.into()),
            metadata: serde_json::Value::Null,
        }
    }
}

impl From<&str> for CapabilityDecl {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for CapabilityDecl {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for CapabilityDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl PartialEq<str> for CapabilityDecl {
    fn eq(&self, other: &str) -> bool {
        self.name == other
    }
}

impl PartialEq<String> for CapabilityDecl {
    fn eq(&self, other: &String) -> bool {
        self.name == *other
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn from_str_creates_name_only() {
        let cap: CapabilityDecl = "github.read_issues".into();
        assert_eq!(cap.name, "github.read_issues");
        assert!(cap.connector.is_none());
        assert!(cap.metadata.is_null());
    }

    #[test]
    fn with_connector_sets_both() {
        let cap = CapabilityDecl::with_connector("send_message", "slack");
        assert_eq!(cap.name, "send_message");
        assert_eq!(cap.connector.as_deref(), Some("slack"));
    }

    #[test]
    fn display_shows_name() {
        let cap = CapabilityDecl::new("github");
        assert_eq!(format!("{cap}"), "github");
    }

    #[test]
    fn eq_str_compares_name() {
        let cap = CapabilityDecl::new("github");
        assert!(cap == *"github");
    }

    #[test]
    fn serde_roundtrip() {
        let cap = CapabilityDecl::with_connector("read", "github");
        let json = serde_json::to_string(&cap).unwrap();
        let parsed: CapabilityDecl = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, parsed);
    }

    #[test]
    fn serde_minimal_skips_null_fields() {
        let cap = CapabilityDecl::new("github");
        let json = serde_json::to_string(&cap).unwrap();
        assert!(!json.contains("connector"));
        assert!(!json.contains("metadata"));
    }

    #[test]
    fn hash_and_eq_work_for_hashset() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(CapabilityDecl::new("github"));
        set.insert(CapabilityDecl::new("github"));
        assert_eq!(set.len(), 1);
        set.insert(CapabilityDecl::new("slack"));
        assert_eq!(set.len(), 2);
    }
}
