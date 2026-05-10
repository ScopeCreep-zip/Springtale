//! Onboarding form descriptors — each platform connector declares the
//! fields its first-run wizard collects.
//!
//! Lives in `springtale-connector` so each connector crate can return its
//! own `PlatformForm` from `ConnectorFactory::onboarding_form()` per the
//! universal-modular-connector-interface principle (plan §F-conn-1):
//! adding a new connector that needs onboarding requires zero edits to
//! `springtale-runtime` or the desktop shell.

use serde::Serialize;

/// A single field the user must fill in for a platform.
#[derive(Debug, Clone, Serialize)]
pub struct FormField {
    /// Stable machine key used as the JSON property name.
    pub name: &'static str,
    /// Human label shown by the frontend.
    pub label: &'static str,
    /// Short hint/help text.
    pub description: &'static str,
    /// Frontend should mask input (password prompt, hidden field).
    pub secret: bool,
    /// Optional default value the user can accept without typing.
    pub default: Option<&'static str>,
    pub required: bool,
    /// Regex pattern the answer must match (OWASP ASVS §5.1.4).
    /// `None` = no format restriction beyond non-empty.
    pub validation: Option<&'static str>,
}

/// One platform the onboarding wizard knows how to set up.
///
/// Each platform connector defines a `'static` instance and returns
/// `Some(&FORM)` from its `ConnectorFactory::onboarding_form()` impl.
/// Non-platform connectors (filesystem, shell, http) return `None` —
/// they have no first-run wizard fields to collect.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformForm {
    /// Stable ID used in `apply_platform` calls.
    pub id: &'static str,
    /// Internal config key (also the connector's `config_key`).
    pub config_key: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    pub description: &'static str,
    pub setup_help: &'static str,
    pub fields: &'static [FormField],
}

impl PlatformForm {
    pub fn field(&self, name: &str) -> Option<&'static FormField> {
        self.fields.iter().find(|f| f.name == name)
    }
}
