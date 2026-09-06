//! Error ID → remediation mapping, shared across all frontends.
//!
//! When a caller sees `[E003]` they can look up guidance (and, for a few
//! errors, trigger an automated repair) without duplicating the table in
//! every UI. Bound to [`crate::error::OperationError::error_id`] so the IDs
//! stay in lockstep with the error enum.

use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::operations::diagnostics::{self, DiagnosticPaths};

/// Static guidance for a single error ID.
#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct FixGuide {
    pub id: &'static str,
    pub title: &'static str,
    pub causes: &'static [&'static str],
    pub suggestions: &'static [&'static str],
    /// Whether [`auto_fix`] can attempt a repair.
    pub has_auto_fix: bool,
}

/// Result of an attempted automated fix.
#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
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
    auto_fix_with_key(error_id, None).await
}

/// [`auto_fix`] with the store key when the caller has the vault
/// unlocked (plan 0.5). Without it, fixers that open the store report
/// that the store is encrypted instead of opening plain.
pub async fn auto_fix_with_key(error_id: &str, encryption_key_hex: Option<&str>) -> FixOutcome {
    let id = error_id.to_ascii_uppercase();
    match id.as_str() {
        "E001" => fix_store_error(encryption_key_hex).await,
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

async fn fix_store_error(encryption_key_hex: Option<&str>) -> FixOutcome {
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

    // Never open plain (plan 0.5): without the key this reports instead.
    match try_open_db(db_path, encryption_key_hex) {
        Ok(()) => outcome
            .push("Database opened successfully. The error may be transient.")
            .succeed(),
        Err(e) => outcome
            .push(format!("Database failed to open: {e}"))
            .push("Possible causes: wrong passphrase, corruption, or a stale lockfile.")
            .push("Restore from backup with `springtale travel restore` if needed."),
    }
}

fn try_open_db(path: &Path, encryption_key_hex: Option<&str>) -> Result<(), String> {
    let Some(key) = encryption_key_hex else {
        return Err("store is encrypted; rerun with the vault unlocked".to_owned());
    };
    springtale_store::backend::sqlite::SqliteBackend::open_encrypted(path, key)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

async fn fix_init_error() -> FixOutcome {
    let mut outcome = FixOutcome::new("E009").push("Running diagnostics...");
    let report = diagnostics::run_default_checks(diagnostics::CallerContext::Cli).await;
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
    // ── Cooperation-layer guidance (COOP-XXXX) ──────────────────────────
    // One entry per `CooperationError` sub-variant so `springtale fix
    // COOP-NNNN` always returns something actionable. Required by plan
    // §16.6 ("every variant has a stable ID and a fix entry").

    // Cadence (COOP-1xxx)
    FixGuide {
        id: "COOP-1001",
        title: "Cadence — tick bus channel closed",
        causes: &[
            "The formation owning the bus was dropped before all subscribers exited.",
            "A panic in the cadence driver task closed the sender side.",
        ],
        suggestions: &[
            "Redeploy the formation via `springtale formation deploy <id>`.",
            "Check `springtale logs` for a preceding panic or supervisor restart.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-1002",
        title: "Cadence — tick sequence wrapped",
        causes: &[
            "A single formation ran long enough for the u64 tick counter to wrap. This should be practically impossible but is guarded anyway.",
        ],
        suggestions: &[
            "Recycle the formation. Wrap-around on u64 at 30 Hz implies uptime on the order of billions of years.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-1003",
        title: "Cadence — subscriber lagged",
        causes: &[
            "An agent ran slower than the tick rate and lost ticks.",
            "The cadence bus channel capacity is too small for the formation's burst workload.",
        ],
        suggestions: &[
            "Drop the tick rate: `springtale config set cadence.rate_hz 15`.",
            "Increase bus capacity in the formation constraints.",
        ],
        has_auto_fix: false,
    },
    // Formation (COOP-2xxx)
    FixGuide {
        id: "COOP-2001",
        title: "Formation — agent not found",
        causes: &["An operation referenced an AgentId that isn't a member of the formation."],
        suggestions: &[
            "List members with `springtale formation show <id>`.",
            "Re-add the agent via `add_formation_member`.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-2002",
        title: "Formation — empty formation",
        causes: &["All members have been removed or died with no recovery."],
        suggestions: &[
            "Add members back: `springtale formation add-member <id> <connector>`.",
            "Or dissolve with `springtale formation dissolve <id>`.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-2003",
        title: "Formation — not viable",
        causes: &["The formation has no operational members (all dead/disconnected)."],
        suggestions: &[
            "Check member health via `springtale formation show <id>`.",
            "Dissolve and redeploy the formation.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-2004",
        title: "Formation — missing required capability",
        causes: &["The formation's intent requires a capability no member provides."],
        suggestions: &[
            "Add a member whose connector declares the missing capability.",
            "Or relax the intent to one the existing capabilities cover.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-2005",
        title: "Formation — context uninitialized",
        causes: &["A formation was constructed but its FormationContext was never set."],
        suggestions: &[
            "This is a library-level bug. File an issue with the formation id and recent actions.",
        ],
        has_auto_fix: false,
    },
    // Momentum (COOP-3xxx)
    FixGuide {
        id: "COOP-3001",
        title: "Momentum — insufficient tier",
        causes: &[
            "An operation was attempted at a momentum tier that doesn't unlock it (e.g. environment writes require Hot).",
        ],
        suggestions: &[
            "Keep the formation running with low interference so momentum climbs.",
            "See docs/guide/cooperation.md for the §7 capability table.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-3002",
        title: "Momentum — capability locked at tier",
        causes: &[
            "The requested capability is gated behind a higher tier (e.g. consensus requires Fever).",
        ],
        suggestions: &[
            "Run more successful ticks to climb tier, or choose a capability available at current tier.",
        ],
        has_auto_fix: false,
    },
    // Awareness (COOP-4xxx)
    FixGuide {
        id: "COOP-4001",
        title: "Awareness — stale neighbor",
        causes: &[
            "The neighbor hasn't reported for many ticks.",
            "Gossip or SWIM dropped the neighbor's updates.",
        ],
        suggestions: &[
            "Check neighbor liveness via the colony canvas or `/formations/{id}` API.",
            "If cross-process: verify chitchat / SWIM seed reachability.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-4002",
        title: "Awareness — gossip bridge disconnected",
        causes: &["The chitchat gossip node lost its transport (usually network)."],
        suggestions: &[
            "For single-process: this shouldn't happen — file an issue.",
            "For cross-process: check UDP connectivity to the seed list.",
        ],
        has_auto_fix: false,
    },
    // Consensus (COOP-5xxx)
    FixGuide {
        id: "COOP-5001",
        title: "Consensus — no override tokens",
        causes: &[
            "An agent tried to override a vote but has already spent all their override tokens.",
        ],
        suggestions: &[
            "Wait for the vote deadline, accept the majority outcome, or dissolve the vote.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-5002",
        title: "Consensus — deadline expired",
        causes: &["The vote timer elapsed before enough votes were cast."],
        suggestions: &[
            "Recycle the vote with a longer deadline.",
            "Check member health — absent voters may be incapacitated.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-5003",
        title: "Consensus — vote not found",
        causes: &[
            "A vote id was referenced that doesn't exist on the formation (typo, wrong formation).",
        ],
        suggestions: &["List open votes in the formation detail API."],
        has_auto_fix: false,
    },
    // Commit (COOP-6xxx)
    FixGuide {
        id: "COOP-6001",
        title: "Commit — barrier failed",
        causes: &[
            "One participant voted abort during Prepare → the whole commit aborts.",
            "Ready phase never completed before the deadline.",
        ],
        suggestions: &[
            "Review each participant's abort reason in the barrier's result map.",
            "Increase the deadline or reduce participant count.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-6002",
        title: "Commit — prepare timed out",
        causes: &["Not every participant reached Ready within the deadline."],
        suggestions: &[
            "Investigate the pending agents' health / load.",
            "Raise the per-tick commit deadline in formation constraints.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-6003",
        title: "Commit — participant dropped",
        causes: &["A barrier participant disappeared mid-commit (task abort / panic)."],
        suggestions: &["Check the supervisor restart log for the dropped agent."],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-6004",
        title: "Commit — agent not a participant",
        causes: &["An agent tried to signal readiness on a barrier it wasn't listed in."],
        suggestions: &[
            "Verify the `begin_commit(participants, ...)` call included every agent that would signal.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-6005",
        title: "Commit — execution failed",
        causes: &["A participant entered Execute phase but its operation errored."],
        suggestions: &[
            "Look at the failing agent's latest action log.",
            "If the same agent fails repeatedly, consider role transformation.",
        ],
        has_auto_fix: false,
    },
    // Interference (COOP-7xxx)
    FixGuide {
        id: "COOP-7001",
        title: "Interference — cross-agent conflict detected",
        causes: &[
            "Two agents wrote to the same workspace key the same tick (ResourceConflict).",
            "An agent acted against another's in-flight action (ActionNegation).",
        ],
        suggestions: &[
            "Inspect the InterferenceEvent for the conflicting agent pair.",
            "Tune the attention broker to separate the agents' workloads.",
        ],
        has_auto_fix: false,
    },
    // Rally (COOP-8xxx)
    FixGuide {
        id: "COOP-8001",
        title: "Rally — no tokens remaining",
        causes: &["The formation has exhausted its rally budget (Monster Hunter cart threshold)."],
        suggestions: &[
            "The formation will escalate to orchestrator intervention.",
            "Check `springtale logs` for the cascade reason.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-8002",
        title: "Rally — cascade threshold exceeded",
        causes: &[
            "Too many agents failed in quick succession; the formation is cascading toward dissolution.",
        ],
        suggestions: &[
            "Pause the formation and investigate recent actions.",
            "Consider sacrifice evaluation to preserve partial operation.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-8003",
        title: "Rally — supervisor panicked",
        causes: &["The rally supervisor task itself panicked — a library bug."],
        suggestions: &["File an issue with the stack trace from `springtale logs`."],
        has_auto_fix: false,
    },
    // Recovery (COOP-9xxx)
    FixGuide {
        id: "COOP-9001",
        title: "Recovery — no path available",
        causes: &["The recovery evaluator found no neighboring agent capable of helping."],
        suggestions: &[
            "Add a member with the required helper capability.",
            "Accept terminal failure and dissolve the formation.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-9002",
        title: "Recovery — cost exceeds budget",
        causes: &[
            "The cheapest recovery path would exhaust the formation's remaining fuel or rally tokens.",
        ],
        suggestions: &[
            "Increase the formation's fuel budget before redeploying.",
            "Reduce recovery ambition — accept degraded operation instead.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-9003",
        title: "Recovery — terminal failure",
        causes: &["An agent reached max quick-fixes (L4D black & white) and died permanently."],
        suggestions: &[
            "Add a replacement member or dissolve the formation.",
            "Review what caused the repeated failures — escalating fragility signals systemic issue.",
        ],
        has_auto_fix: false,
    },
    // ── Handoff (§20) ───────────────────────────────────────────────
    FixGuide {
        id: "COOP-A001",
        title: "Handoff — no capable receiver",
        causes: &[
            "Sender selected `HandoffType::Direct` for a capability no operational member holds.",
            "All members holding the required capability are degraded or dead.",
        ],
        suggestions: &[
            "Add a member whose capability set covers the required action.",
            "Switch to `HandoffType::Flexible` so any capable member can claim the work.",
            "Verify the capability name matches the connector's manifest declaration exactly.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-A002",
        title: "Handoff — payload expired",
        causes: &[
            "Direct-inbox queue depth exceeded TTL before the receiver picked it up.",
            "FlexibleChain pool starved — no one stole the work before expiry.",
        ],
        suggestions: &[
            "Increase the handoff TTL on the originating action.",
            "Add another member with the same capability so steal attempts have more candidates.",
            "Check the receiving member's liveness — Suspect/Down members don't drain their inbox.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-A003",
        title: "Handoff — return obligation unmet",
        causes: &[
            "Receiver claimed a `requires_response: true` task and dropped without acking.",
            "Receiver completed work but the ack message lost to a transient transport failure.",
        ],
        suggestions: &[
            "Inspect the receiver's recent dispatches via `springtale events list --connector <name>`.",
            "Check the bus dispatcher's log for `ack_dispatcher` warnings around the same tick.",
            "If recurring, mark the action `requires_response: false` and rely on eventual consistency.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-A004",
        title: "Handoff — payload serialization failed",
        causes: &[
            "A custom action's input contains a Rust type that doesn't implement `Serialize`.",
            "Payload contains a value (e.g. infinity, NaN) JSON can't represent.",
        ],
        suggestions: &[
            "Restrict action inputs to standard JSON-compatible types (string/number/bool/array/object).",
            "Add a custom `Serialize` impl that maps the problematic values to a safe representation.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-A005",
        title: "Handoff — payload deserialization failed",
        causes: &[
            "Wire format from sender doesn't match the receiver's expected shape.",
            "Schema drift — sender uses a newer manifest version than the receiver.",
        ],
        suggestions: &[
            "Reload both connectors via `springtale fix COOP-C003` so they share manifest version.",
            "If cross-process: check the sender and receiver run the same connector binary version.",
        ],
        has_auto_fix: false,
    },
    // ── Pacing (§22) ────────────────────────────────────────────────
    FixGuide {
        id: "COOP-B001",
        title: "Pacing — violation",
        causes: &[
            "An agent exceeded its per-phase quota (Preparation/Active/Peak/Recovery/Disruption).",
            "Pacing manager transition didn't propagate to the agent's rate limiter in time.",
        ],
        suggestions: &[
            "Wait one cooperation tick — the agent will pick up the new phase's quota next iteration.",
            "If persistent, check the formation's momentum tier — pacing quotas are tier-dependent.",
            "Review the connector's emitted rate; some external APIs need stricter quotas than the default.",
        ],
        has_auto_fix: false,
    },
    // ── Cross-cutting (aggregate) ───────────────────────────────────
    FixGuide {
        id: "COOP-C000",
        title: "Store — backend error surfaced through cooperation",
        causes: &[
            "Underlying SQLite/redb backend returned an error during a cooperation operation.",
        ],
        suggestions: &[
            "Inspect `springtale doctor` for store-level diagnostics.",
            "Verify the vault is unlocked and the store path is writable.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-C001",
        title: "Gossip — cross-process state propagation failure",
        causes: &[
            "Chitchat node lost connectivity to its seed peers.",
            "SWIM liveness monitor flagged the peer process as Down.",
        ],
        suggestions: &[
            "Verify the seed list in `~/.springtale/cooperation.toml` includes a reachable peer.",
            "Single-process deployments don't use gossip — set `cross_process = false` if not federating.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-C002",
        title: "Liveness — peer-down detected",
        causes: &[
            "A federated peer process stopped responding to SWIM ping/ack within the suspect window.",
        ],
        suggestions: &[
            "Check the remote process is still running.",
            "Inspect firewall / NAT rules — SWIM uses UDP and can be blocked silently.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-C003",
        title: "Capability bridge — dispatch failure",
        causes: &[
            "Connector registry doesn't have the named connector installed.",
            "Capability check denied the action (manifest's declared capabilities don't include it).",
            "Connector's wasm sandbox tripped a tier-gated limit.",
        ],
        suggestions: &[
            "Run `springtale connector list` to confirm the connector is installed and enabled.",
            "Reload the connector with `springtale connector reload <name>` (G4 hot-reload path).",
            "Check the connector's manifest declares the action's required capability.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-C004",
        title: "Role — registry lookup failure",
        causes: &[
            "A formation tried to transform a member into a role the role registry doesn't know about.",
            "Community role contributed by a connector manifest, but the connector was uninstalled.",
        ],
        suggestions: &[
            "Reinstall the connector that contributed the role.",
            "Use a built-in role (General / Information / Support) as a fallback.",
        ],
        has_auto_fix: false,
    },
    FixGuide {
        id: "COOP-C005",
        title: "Invariant — internal consistency violated",
        causes: &[
            "A cooperation primitive observed state that shouldn't be reachable through the public API.",
            "Concurrent modification raced past a guard the type system was supposed to enforce.",
        ],
        suggestions: &[
            "Capture the surrounding tick's events via `springtale trace --tail`.",
            "File an issue with the trace — this class of error indicates a Springtale bug, not user error.",
        ],
        has_auto_fix: false,
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

    #[test]
    fn guides_cover_every_cooperation_error_variant() {
        // Per COOPERATION_IMPLEMENTATION_PLAN.md §16.6 — every
        // `CooperationError` variant must have a `springtale fix <id>`
        // entry. These IDs mirror the `#[error("COOP-NNNN: ...")]`
        // annotations in `crates/springtale-cooperation/src/error/*.rs`
        // and the `code()` accessor each error enum exposes (J1).
        for id in [
            // Cadence
            "COOP-1001",
            "COOP-1002",
            "COOP-1003",
            // Formation
            "COOP-2001",
            "COOP-2002",
            "COOP-2003",
            "COOP-2004",
            "COOP-2005",
            // Momentum
            "COOP-3001",
            "COOP-3002",
            // Awareness
            "COOP-4001",
            "COOP-4002",
            // Consensus
            "COOP-5001",
            "COOP-5002",
            "COOP-5003",
            // Commit
            "COOP-6001",
            "COOP-6002",
            "COOP-6003",
            "COOP-6004",
            "COOP-6005",
            // Interference
            "COOP-7001",
            // Rally
            "COOP-8001",
            "COOP-8002",
            "COOP-8003",
            // Recovery
            "COOP-9001",
            "COOP-9002",
            "COOP-9003",
            // Handoff (J1)
            "COOP-A001",
            "COOP-A002",
            "COOP-A003",
            "COOP-A004",
            "COOP-A005",
            // Pacing (J1)
            "COOP-B001",
            // Cross-cutting aggregate variants (J1)
            "COOP-C000",
            "COOP-C001",
            "COOP-C002",
            "COOP-C003",
            "COOP-C004",
            "COOP-C005",
        ] {
            assert!(
                lookup(id).is_some(),
                "missing springtale-fix guide for cooperation error {id}"
            );
        }
    }

    /// J1 — every cooperation error variant's `code()` must round-trip
    /// through the fix-guide registry. Exercises each `code()`
    /// implementation against the registry to catch ID drift.
    #[test]
    fn error_code_accessor_matches_registry() {
        use springtale_cooperation::error::{
            AwarenessError, CadenceError, CommitError, ConsensusError, FormationError,
            HandoffError, InterferenceError, MomentumError, PacingError, RallyError, RecoveryError,
        };

        let cadence_codes = [
            CadenceError::ChannelClosed.code(),
            CadenceError::SequenceWrap.code(),
            CadenceError::Lagged { lagged: 0 }.code(),
        ];
        for code in cadence_codes {
            assert!(lookup(code).is_some(), "no guide for cadence {code}");
        }
        let formation = [
            FormationError::Empty.code(),
            FormationError::NotViable(String::new()).code(),
            FormationError::ContextUninit.code(),
        ];
        for code in formation {
            assert!(lookup(code).is_some(), "no guide for formation {code}");
        }
        let momentum =
            [
                MomentumError::CapabilityLocked(
                    springtale_cooperation::momentum::MomentumTier::Cold,
                )
                .code(),
            ];
        for code in momentum {
            assert!(lookup(code).is_some(), "no guide for momentum {code}");
        }
        let awareness = [AwarenessError::GossipDisconnected.code()];
        for code in awareness {
            assert!(lookup(code).is_some(), "no guide for awareness {code}");
        }
        let consensus = [ConsensusError::Timeout.code()];
        for code in consensus {
            assert!(lookup(code).is_some(), "no guide for consensus {code}");
        }
        let commit = [
            CommitError::BarrierFailed(String::new()).code(),
            CommitError::PrepareTimeout { pending: 0 }.code(),
        ];
        for code in commit {
            assert!(lookup(code).is_some(), "no guide for commit {code}");
        }
        let interference = [InterferenceError::Detected(String::new()).code()];
        for code in interference {
            assert!(lookup(code).is_some(), "no guide for interference {code}");
        }
        let rally = [
            RallyError::Exhausted.code(),
            RallyError::CascadeEscalating.code(),
            RallyError::SupervisorPanic.code(),
        ];
        for code in rally {
            assert!(lookup(code).is_some(), "no guide for rally {code}");
        }
        let recovery = [RecoveryError::BudgetExceeded.code()];
        for code in recovery {
            assert!(lookup(code).is_some(), "no guide for recovery {code}");
        }
        let handoff = [
            HandoffError::PayloadExpired.code(),
            HandoffError::UnmetObligation.code(),
            HandoffError::SerializeDeposit(String::new()).code(),
            HandoffError::DeserializeDeposit(String::new()).code(),
        ];
        for code in handoff {
            assert!(lookup(code).is_some(), "no guide for handoff {code}");
        }
        let pacing = [PacingError::Violation(String::new()).code()];
        for code in pacing {
            assert!(lookup(code).is_some(), "no guide for pacing {code}");
        }
    }
}
