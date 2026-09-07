//! `springtale doctor` — thin CLI wrapper around the diagnostics operation.
//!
//! All check logic lives in `springtale_runtime::operations::diagnostics`.
//! This file only renders the report for a terminal.

use anyhow::Result;

use springtale_runtime::operations::diagnostics::{
    self, CallerContext, Check, DiagnosticPaths, Report, Severity,
};

use crate::output;
use crate::store::{PassphraseOpts, derive_db_key_hex};

pub async fn run(opts: &PassphraseOpts, json_out: bool) -> Result<()> {
    // The integrity check needs the store key; derive it from the
    // passphrase rather than reporting "vault locked". A first run has
    // no database yet, so do not prompt for one then.
    let paths = DiagnosticPaths::default();
    let key = if paths.database.exists() {
        Some(derive_db_key_hex(opts)?)
    } else {
        None
    };

    // The whole report is rendered in one go so `--json` can hand back
    // the serialized `Report` instead — the header used to be printed
    // before the checks even ran, which left JSON output unparseable.
    let report = diagnostics::run_checks(&paths, key.as_deref(), CallerContext::Cli).await;
    output::emit(json_out, &report, render)
}

fn render(report: &Report) -> String {
    let mut out = String::from("Springtale Doctor\n=================\n\n");
    for check in &report.checks {
        out.push_str(&render_check(check));
    }
    out.push('\n');
    let issues = report.issue_count();
    if issues == 0 {
        out.push_str("All checks passed. Springtale is ready to run.");
    } else {
        out.push_str(&format!(
            "{issues} issue{} found. Fix the items above and run `springtale doctor` again.",
            if issues == 1 { "" } else { "s" }
        ));
    }
    out
}

fn render_check(check: &Check) -> String {
    let tag = match check.severity {
        Severity::Ok => "[OK]  ",
        Severity::Warn => "[WARN]",
        Severity::Fail => "[FAIL]",
    };
    let mut out = format!("{tag} {}\n", check.label);
    if let Some(detail) = &check.detail {
        out.push_str(&format!("       {detail}\n"));
    }
    if let Some(hint) = &check.fix_hint {
        out.push_str(&format!("       {hint}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    fn report() -> Report {
        Report {
            checks: vec![
                Check {
                    id: "config.exists",
                    label: "Config file present".to_owned(),
                    severity: Severity::Ok,
                    detail: None,
                    fix_hint: None,
                },
                Check {
                    id: "vault.exists",
                    label: "Vault present".to_owned(),
                    severity: Severity::Fail,
                    detail: Some("no vault at ~/.springtale/vault.age".to_owned()),
                    fix_hint: Some("run `springtale init`".to_owned()),
                },
            ],
        }
    }

    #[test]
    fn test_doctor_json_shape_is_a_checks_envelope() {
        let out = json_value(&report());
        assert_eq!(key_set(&out), ["checks"]);
        assert!(out["checks"].is_array());
        assert_eq!(crate::output::array(&out, "checks").len(), 2);
    }

    #[test]
    fn test_doctor_check_json_shape_carries_all_five_fields() {
        let out = json_value(&report());
        let failing = &out["checks"][1];
        assert_eq!(
            key_set(failing),
            ["detail", "fix_hint", "id", "label", "severity"]
        );
        assert!(failing["id"].is_string());
        assert!(failing["label"].is_string());
        assert!(failing["severity"].is_string());
        assert!(failing["detail"].is_string());
        assert!(failing["fix_hint"].is_string());
    }

    #[test]
    fn test_doctor_severity_serializes_lowercase_and_nulls_stay_present() {
        let out = json_value(&report());
        assert_eq!(out["checks"][0]["severity"], "ok");
        assert_eq!(out["checks"][1]["severity"], "fail");
        // An unset detail is null, not a missing key: a consumer can
        // index it without guessing.
        assert!(out["checks"][0]["detail"].is_null());
        assert!(out["checks"][0]["fix_hint"].is_null());
        assert_eq!(key_set(&out["checks"][0]).len(), 5);
    }

    #[test]
    fn test_doctor_human_render_reports_the_same_issue_count() {
        let report = report();
        assert_eq!(report.issue_count(), 1);
        let text = render(&report);
        assert!(text.contains("[OK]"));
        assert!(text.contains("[FAIL] Vault present"));
        assert!(text.contains("1 issue found"));
    }
}
