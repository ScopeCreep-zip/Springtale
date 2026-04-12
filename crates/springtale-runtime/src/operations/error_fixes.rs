//! Error ID → remediation mapping, shared across all frontends.
//!
//! When a caller sees `[E003]` they can look up guidance (and, for a few
//! errors, trigger an automated repair) without duplicating the table in
//! every UI. Bound to [`crate::error::OperationError::error_id`] so the IDs
//! stay in lockstep with the error enum.

use std::path::Path;

use serde::Serialize;

use crate::operations::diagnostics::{self, DiagnosticPaths};

/// Static guidance for a single error ID.
#[derive(Debug, Clone, Serialize)]
pub struct FixGuide {
    pub id: &'static str,
    pub title: &'static str,
    pub causes: &'static [&'static str],
    pub suggestions: &'static [&'static str],
    /// Whether [`auto_fix`] can attempt a repair.
    pub has_auto_fix: bool,
}

/// Result of an attempted automated fix.
#[derive(Debug, Clone, Serialize)]
pub struct FixOutcome {
    pub id: &'static str,
    pub success: bool,
    pub messages: Vec<String>,
}

impl FixOutcome {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            success: false,
            messages: Vec::new(),
        }
    }

    fn push(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(msg.into());
        self
    }

    fn succeed(mut self) -> Self {
        self.success = true;
        self
    }
}

/// Look up the `FixGuide` for an error ID. Matching is case-insensitive.
///
/// Returns `None` if the ID is unknown — callers should list [`all_guides`].
pub fn lookup(error_id: &str) -> Option<&'static FixGuide> {
    let id = error_id.to_ascii_uppercase();
    GUIDES.iter().find(|g| g.id == id)
}

/// All known guides, ordered by ID.
pub fn all_guides() -> &'static [FixGuide] {
    GUIDES
}

/// Attempt an automated fix. Only a few errors have auto-fixes; most just
/// return guidance. The CLI/Tauri frontends should call [`lookup`] first,
/// show guidance, and only invoke [`auto_fix`] when the user opts in.
pub async fn auto_fix(error_id: &str) -> FixOutcome {
    let id = error_id.to_ascii_uppercase();
    match id.as_str() {
        "E001" => fix_store_error().await,
        "E009" => fix_init_error().await,
        other => FixOutcome::new(interned_id(other)).push(format!(
            "No automated fix is available for {other}. Follow the suggestions above."
        )),
    }
}

/// Map a user-supplied string back to the canonical static ID or fall back
/// to a short literal so `FixOutcome` remains `'static`-friendly.
fn interned_id(candidate: &str) -> &'static str {
    GUIDES
        .iter()
        .find(|g| g.id == candidate)
        .map(|g| g.id)
        .unwrap_or("E???")
}

// ---------- Automated fixers ----------

async fn fix_store_error() -> FixOutcome {
    let mut outcome = FixOutcome::new("E001");
    let paths = DiagnosticPaths::default();
    let db_path = &paths.database;

    if !db_path.exists() {
        return outcome
            .push(format!("Database not found at {}", db_path.display()))
            .push("Run `springtale init` to create the database.".to_owned());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(db_path) {
            let mode = meta.permissions().mode() & 0o777;
            if mode != 0o600 {
                outcome = outcome.push(format!(
                    "Tightening database permissions: 0o{mode:03o} → 0o600"
                ));
                let perms = std::fs::Permissions::from_mode(0o600);
                match std::fs::set_permissions(db_path, perms) {
                    Ok(()) => {
                        outcome = outcome.push("Permissions updated.");
                        return outcome.succeed();
                    }
                    Err(e) => {
                        return outcome.push(format!("chmod failed: {e}"));
                    }
                }
            }
        }
    }

    match try_open_db(db_path) {
        Ok(()) => outcome
            .push("Database opened successfully. The error may be transient.")
            .succeed(),
        Err(e) => outcome
            .push(format!("Database failed to open: {e}"))
            .push("Possible causes: wrong passphrase, corruption, or a stale lockfile.")
            .push("Restore from backup with `springtale travel restore` if needed."),
    }
}

fn try_open_db(path: &Path) -> Result<(), String> {
    springtale_store::backend::sqlite::SqliteBackend::open(path)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

async fn fix_init_error() -> FixOutcome {
    let mut outcome = FixOutcome::new("E009")
        .push("Running diagnostics...");
    let report = diagnostics::run_default_checks().await;
    for check in &report.checks {
        outcome
            .messages
            .push(format!("{:?}: {}", check.severity, check.label));
    }
    if report.has_failures() {
        outcome.push("Fix the failures above, then retry.")
    } else {
        outcome
            .push("No blocking diagnostics found. Startup failure may be transient.")
            .succeed()
    }
}

// ---------- Static guidance table ----------

static GUIDES: &[FixGuide] = &[
    FixGuide {
        id: "E001",
        title: "Store error — database access failure",
        causes: &[
            "Database file missing",
            "Wrong file permissions (should be 0o600)",
            "Wrong passphrase or encrypted with a different key",
            "Another process holds a lock on the file",
        ],
        suggestions: &[
            "Run `springtale init` if the database is missing.",
            "Run `springtale fix E001` to auto-correct permissions.",
            "Check `SPRINGTALE_PASSPHRASE` matches the vault passphrase.",
            "Restore from a backup with `springtale travel restore`.",
        ],
        has_auto_fix: true,
    },
    FixGuide {
        id: "E002",
        title: "Rule error — invalid rule definition",
        causes: &[
            "Invalid TOML syntax in rule file",
            "Missing required fields (name, trigger, actions)",
            "Invalid trigger type or expression",
            "Too many actions (max 100 per rule)",
            "Chain depth exceeded (max 4 levels)",
        ],
        suggestions: &[
            "Validate with `springtale rule add <file>` and read the error message.",
            "See docs/guide/rules.md for the TOML schema.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "E003",
        title: "Connector error — execution failure",
        causes: &[
            "Missing or invalid API token",
            "Connector is disabled",
            "Network unreachable",
            "Rate limited by external API",
        ],
        suggestions: &[
            "Check `springtale connector list` for status.",
            "Run `springtale doctor` to diagnose connectivity issues.",
            "Move secrets into the vault rather than editing TOML by hand.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "E004",
        title: "Formation error — multi-agent group issue",
        causes: &[
            "Formation has no operational members",
            "Momentum tier insufficient for requested operation",
            "AI adapter not available for orchestration (requires Fever momentum)",
        ],
        suggestions: &["Check formation status in the dashboard or API."],
        has_auto_fix: false,
    },
    FixGuide {
        id: "E005",
        title: "Not found — missing resource",
        causes: &["The requested rule, connector, or formation doesn't exist."],
        suggestions: &[
            "Check the ID/name spelling.",
            "List available resources: `springtale rule list`, `springtale connector list`.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "E006",
        title: "Validation error — invalid input",
        causes: &[
            "Manifest has a wildcard host in NetworkOutbound capability",
            "Toxic capability pair (e.g. KeychainRead + NetworkOutbound)",
            "Path parameter exceeds 256 characters",
            "Invalid connector or rule name",
        ],
        suggestions: &["Read the specific validation message in the error output."],
        has_auto_fix: false,
    },
    FixGuide {
        id: "E007",
        title: "Serialization error — JSON/TOML parsing failure",
        causes: &[
            "Malformed JSON in an API request body",
            "Invalid TOML in a rule file",
            "Corrupted data in the database",
        ],
        suggestions: &["Validate your input with a JSON/TOML linter before submitting."],
        has_auto_fix: false,
    },
    FixGuide {
        id: "E008",
        title: "AI error — adapter failure",
        causes: &[
            "AI adapter not configured",
            "API key missing or invalid",
            "AI service unreachable",
            "Response too large (>10 MiB)",
            "Input blocked by sanitization policy",
        ],
        suggestions: &[
            "Without AI: Springtale still works with the NoopAdapter — every command runs.",
            "Ollama: `ollama serve` must be running on localhost:11434.",
            "OpenAI/Anthropic: move the API key into the vault.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "E009",
        title: "Initialization error — startup failure",
        causes: &[
            "Vault passphrase is wrong or not provided",
            "Database file is missing or corrupted",
            "Config file has syntax errors",
            "Required port is already in use",
        ],
        suggestions: &[
            "Run `springtale fix E009` to run full diagnostics.",
            "Set `SPRINGTALE_PASSPHRASE` if you've moved to a non-interactive launcher.",
        ],
        has_auto_fix: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(lookup("e001").map(|g| g.id), Some("E001"));
        assert_eq!(lookup("E001").map(|g| g.id), Some("E001"));
        assert!(lookup("E999").is_none());
    }

    #[test]
    fn guides_cover_every_operation_error_variant() {
        for id in [
            "E001", "E002", "E003", "E004", "E005", "E006", "E007", "E008", "E009",
        ] {
            assert!(lookup(id).is_some(), "missing guide for {id}");
        }
    }
}
