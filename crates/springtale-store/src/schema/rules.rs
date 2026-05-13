use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use springtale_core::rule::types::Rule;

/// Row type for the `rules` table.
///
/// Wraps the core `Rule` type with persistence metadata.
/// The rule itself is stored as TOML in `rule_toml`.
///
/// Internal storage type — never crosses IPC, so no `specta::Type`
/// derive (and couldn't have one anyway: `Rule` is intentionally
/// non-`Type` per the rule-module policy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleRow {
    /// The rule definition (deserialized from TOML).
    pub rule: Rule,
    /// When the rule was created.
    pub created_at: DateTime<Utc>,
    /// When the rule was last updated.
    pub updated_at: DateTime<Utc>,
}
