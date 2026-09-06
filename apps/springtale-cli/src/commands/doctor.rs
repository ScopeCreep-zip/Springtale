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
