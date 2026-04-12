//! Diagnostic checks — `springtale doctor` and its frontends.
//!
//! Returns a list of `Check` results that any frontend (CLI, Tauri, web)
//! can render. Frontends must not re-implement check logic.
//!
//! Checks are grouped to mirror the OpenClaw parity row 16 ("config doctor"):
//! config file, vault, database, data directory, API port, connector wiring.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Severity of a diagnostic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Everything is fine.
    Ok,
    /// Non-fatal — worth the user's attention.
    Warn,
    /// Fatal — the system will not function correctly.
    Fail,
}

/// One diagnostic finding.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// Short identifier (e.g. `"config.exists"`).
    pub id: &'static str,
    /// Human-readable label.
    pub label: String,
    pub severity: Severity,
    /// Longer description / detected value.
    pub detail: Option<String>,
    /// Suggested remediation, if any.
    pub fix_hint: Option<String>,
}

impl Check {
    fn ok(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            severity: Severity::Ok,
            detail: None,
            fix_hint: None,
        }
    }

    fn warn(id: &'static str, label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            severity: Severity::Warn,
            detail: None,
            fix_hint: Some(hint.into()),
        }
    }

    fn fail(id: &'static str, label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            severity: Severity::Fail,
            detail: None,
            fix_hint: Some(hint.into()),
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Who is running the diagnostics — determines which checks make sense.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerContext {
    /// CLI: daemon is NOT running. Port check is meaningful.
    Cli,
    /// API: called via GET /diagnostics inside the running daemon.
    /// Port check would always false-alarm (we own the port).
    Api,
}

/// Aggregated result of running all diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    /// Count failures and warnings (Ok is excluded).
    pub fn issue_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.severity != Severity::Ok)
            .count()
    }

    pub fn has_failures(&self) -> bool {
        self.checks.iter().any(|c| c.severity == Severity::Fail)
    }
}

/// Run all diagnostic checks using default paths.
pub async fn run_default_checks(context: CallerContext) -> Report {
    run_checks(&DiagnosticPaths::default(), context).await
}

/// Paths that diagnostics inspect. Overridable for tests and alt installs.
#[derive(Debug, Clone)]
pub struct DiagnosticPaths {
    pub config: PathBuf,
    pub vault: PathBuf,
    pub database: PathBuf,
    pub data_dir: PathBuf,
}

impl Default for DiagnosticPaths {
    fn default() -> Self {
        Self {
            config: PathBuf::from("springtale.toml"),
            vault: springtale_store::paths::default_vault_path(),
            database: springtale_store::paths::default_db_path(),
            data_dir: springtale_store::paths::data_dir(),
        }
    }
}

/// Run all diagnostics against the supplied paths.
pub async fn run_checks(paths: &DiagnosticPaths, context: CallerContext) -> Report {
    let mut checks = Vec::new();

    let config_text = check_config(&paths.config, &mut checks);
    check_vault(&paths.vault, &mut checks);
    check_database(&paths.database, &mut checks);
    check_data_dir(&paths.data_dir, &mut checks);
    if context == CallerContext::Cli {
        check_api_port(&mut checks);
    }
    check_connectors_section(config_text.as_deref(), &mut checks);

    Report { checks }
}

/// Returns the config file text on success so later checks can reuse it.
fn check_config(path: &Path, checks: &mut Vec<Check>) -> Option<String> {
    if !path.exists() {
        checks.push(
            Check::fail(
                "config.exists",
                format!("Config file not found: {}", path.display()),
                "Run: springtale init",
            ),
        );
        return None;
    }

    match std::fs::read_to_string(path) {
        Ok(text) => {
            checks.push(Check::ok("config.exists", format!("Config file: {}", path.display())));
            // Detect plaintext secrets — heuristic: any `*_token`/`*_key`/`*secret*`
            // assignment whose value isn't a placeholder.
            if contains_plaintext_secret(&text) {
                checks.push(Check::warn(
                    "config.plaintext_secret",
                    "Config file contains secrets in plaintext",
                    "Move secrets to the vault (see docs/current-arch/SECURITY.md §2)",
                ));
            } else {
                checks.push(Check::ok(
                    "config.plaintext_secret",
                    "No plaintext secrets detected in config",
                ));
            }
            Some(text)
        }
        Err(e) => {
            checks.push(
                Check::fail(
                    "config.readable",
                    "Cannot read config file",
                    "Check file permissions",
                )
                .with_detail(e.to_string()),
            );
            None
        }
    }
}

fn check_vault(path: &Path, checks: &mut Vec<Check>) {
    if !path.exists() {
        checks.push(Check::fail(
            "vault.exists",
            format!("Vault not found: {}", path.display()),
            "Run: springtale init",
        ));
        return;
    }
    checks.push(Check::ok("vault.exists", format!("Vault: {}", path.display())));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode() & 0o777;
            if mode == 0o600 {
                checks.push(Check::ok(
                    "vault.permissions",
                    "Vault permissions: 0o600 (owner read/write only)",
                ));
            } else {
                checks.push(
                    Check::fail(
                        "vault.permissions",
                        format!("Vault permissions: 0o{mode:03o} (should be 0o600)"),
                        format!("chmod 600 {}", path.display()),
                    )
                    .with_detail(format!("current mode: 0o{mode:03o}")),
                );
            }
        }
    }
}

fn check_database(path: &Path, checks: &mut Vec<Check>) {
    if !path.exists() {
        checks.push(Check::fail(
            "db.exists",
            format!("Database not found: {}", path.display()),
            "Run: springtale init",
        ));
        return;
    }
    checks.push(Check::ok("db.exists", format!("Database: {}", path.display())));

    match springtale_store::backend::sqlite::SqliteBackend::open(path) {
        Ok(_) => checks.push(Check::ok("db.integrity", "Database integrity: valid")),
        Err(e) => {
            checks.push(
                Check::fail(
                    "db.integrity",
                    format!("Database integrity check failed: {e}"),
                    "The database may be corrupted or encrypted with a different key. \
                     Restore from backup with `springtale travel restore`.",
                )
                .with_detail(e.to_string()),
            );
        }
    }
}

fn check_data_dir(path: &Path, checks: &mut Vec<Check>) {
    if path.exists() {
        checks.push(Check::ok(
            "data_dir.exists",
            format!("Data directory: {}", path.display()),
        ));
    } else {
        checks.push(Check::fail(
            "data_dir.exists",
            format!("Data directory not found: {}", path.display()),
            "Run: springtale init",
        ));
    }
}

fn check_api_port(checks: &mut Vec<Check>) {
    let bind_addr = "127.0.0.1:8080";
    match std::net::TcpListener::bind(bind_addr) {
        Ok(_) => checks.push(Check::ok(
            "api.port_free",
            format!("API port: {bind_addr} available"),
        )),
        Err(_) => checks.push(Check::warn(
            "api.port_free",
            format!("API port: {bind_addr} already in use"),
            "Another springtaled may be running, or another service holds the port. \
             Change [api] bind in springtale.toml if needed.",
        )),
    }
}

fn check_connectors_section(config_text: Option<&str>, checks: &mut Vec<Check>) {
    let Some(text) = config_text else {
        return;
    };
    const CHAT_SECTIONS: &[&str] = &[
        "[telegram]",
        "[discord]",
        "[slack]",
        "[signal]",
        "[nostr]",
        "[irc]",
    ];
    if CHAT_SECTIONS.iter().any(|s| text.contains(s)) {
        checks.push(Check::ok(
            "config.chat_connectors",
            "Chat connectors: configured",
        ));
    } else {
        checks.push(Check::warn(
            "config.chat_connectors",
            "No chat connectors configured",
            "Add a [telegram], [discord], [slack], or [signal] section to springtale.toml",
        ));
    }
}

/// Very narrow plaintext-secret detector used by diagnostics.
///
/// Matches keys that end in `_token`, `_key`, or `_secret` (case-insensitive)
/// with a non-placeholder value. "Placeholder" means any of the conventional
/// template markers we emit in our own starters.
fn contains_plaintext_secret(text: &str) -> bool {
    const PLACEHOLDERS: &[&str] = &["YOUR_", "REPLACE_", "CHANGEME", "xxx"];
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(eq) = trimmed.find('=') else {
            continue;
        };
        let (key, value) = trimmed.split_at(eq);
        let key_lower = key.trim().to_ascii_lowercase();
        let looks_secret = key_lower.ends_with("_token")
            || key_lower.ends_with("_key")
            || key_lower.ends_with("_secret")
            || key_lower == "passphrase";
        if !looks_secret {
            continue;
        }
        let value = value.trim_start_matches('=').trim();
        let value_unquoted = value.trim_matches(|c| c == '"' || c == '\'');
        if value_unquoted.is_empty() {
            continue;
        }
        if PLACEHOLDERS
            .iter()
            .any(|p| value_unquoted.contains(p) || value_unquoted.eq_ignore_ascii_case(p))
        {
            continue;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_secret_detects_real_token() {
        let toml = r#"
            [telegram]
            bot_token = "1234567890:ABCDEFGH"
        "#;
        assert!(contains_plaintext_secret(toml));
    }

    #[test]
    fn plaintext_secret_ignores_placeholders() {
        let toml = r#"
            [telegram]
            bot_token = "YOUR_BOT_TOKEN"
        "#;
        assert!(!contains_plaintext_secret(toml));
    }

    #[test]
    fn plaintext_secret_ignores_comments() {
        let toml = r#"
            # bot_token = "1234567890:ABCDEFGH"
        "#;
        assert!(!contains_plaintext_secret(toml));
    }

    #[test]
    fn report_issue_count_excludes_ok() {
        let report = Report {
            checks: vec![
                Check::ok("a", "ok"),
                Check::warn("b", "warn", "hint"),
                Check::fail("c", "fail", "hint"),
            ],
        };
        assert_eq!(report.issue_count(), 2);
        assert!(report.has_failures());
    }
}
