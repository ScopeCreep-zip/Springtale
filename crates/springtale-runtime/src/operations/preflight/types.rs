//! Preflight types — the report a frontend renders.
//!
//! Architecture: every check classification (blocking vs warning vs
//! verified) is decided in Rust. The frontend renders the report;
//! it never decides whether something is "OK enough to deploy."
//! Same shape feeds the IPC channel and the HTTP route so the
//! desktop and the dashboard render identical checklists.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Severity tier for an individual preflight check.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreflightStatus {
    /// 🔴 Deploy is blocked. The bot literally cannot run without
    /// resolving this. Example: required secret missing.
    Blocking,
    /// 🟡 Deploy is allowed with a confirmation dialog. The bot
    /// will run but probably not as intended. Example: AI configured
    /// but model name unverified.
    Warning,
    /// 🟢 Verified working. Example: token format-valid, connector
    /// loaded, allow-list includes host.
    Verified,
    /// ⏳ Check is still in flight (async probes). Frontend keeps the
    /// row but renders a spinner.
    Pending,
}

/// One row in the preflight report.
#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct PreflightItem {
    /// Stable id for the row (`"secret_present:bot_token"`,
    /// `"connector_loaded:connector-telegram"`). Frontends can
    /// deduplicate / scroll-to-item by this.
    pub id: String,
    /// Human-readable label rendered to the survivor (one line).
    pub label: String,
    pub status: PreflightStatus,
    /// Why the row failed when `status` is `Blocking` or `Warning`,
    /// or extra confirmation text when `Verified`. Plain text.
    pub detail: Option<String>,
    /// Optional "fix this" hint — server-side recommendation
    /// rendered as an inline action. Frontend may turn it into a
    /// button (e.g. "Configure AI" → opens AiConfigPanel).
    pub fix_hint: Option<PreflightFix>,
}

/// What the frontend should offer to resolve a failing check.
/// `kind` is a stable enum so the UI maps to the right panel.
#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreflightFix {
    /// User needs to fill in a recipe input — frontend should scroll
    /// the matching field into view + focus.
    FocusInput { input_id: String },
    /// User needs to configure an AI provider (no AI:global yet).
    OpenAiConfig,
    /// Connector isn't loaded and needs setup before deploy.
    OpenConnectorConfig { connector_name: String },
    /// Generic message — show as plain text, no action.
    Note { message: String },
}

/// Full preflight report — the response body for the `preflight_recipe`
/// command / `/recipes/{id}/preflight` route.
#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct PreflightReport {
    pub recipe_id: String,
    pub items: Vec<PreflightItem>,
    /// Convenience aggregate the frontend uses to enable/disable the
    /// Deploy button without iterating through items itself.
    pub deployable: bool,
    /// `true` when at least one item is `Warning` and none are
    /// `Blocking` — frontend prompts the user for confirmation
    /// before allowing Deploy.
    pub has_warnings: bool,
}

impl PreflightReport {
    pub fn from_items(recipe_id: String, items: Vec<PreflightItem>) -> Self {
        let has_blocking = items
            .iter()
            .any(|i| matches!(i.status, PreflightStatus::Blocking));
        let has_warnings = items
            .iter()
            .any(|i| matches!(i.status, PreflightStatus::Warning));
        Self {
            recipe_id,
            items,
            deployable: !has_blocking,
            has_warnings,
        }
    }
}
