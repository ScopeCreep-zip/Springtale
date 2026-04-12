//! `springtale doctor` — thin CLI wrapper around the diagnostics operation.
//!
//! All check logic lives in `springtale_runtime::operations::diagnostics`.
//! This file only renders the report for a terminal.

use anyhow::Result;

use springtale_runtime::operations::diagnostics::{self, Check, Report, Severity};

pub async fn run() -> Result<()> {
    println!("Springtale Doctor");
    println!("=================\n");

    let report = diagnostics::run_default_checks().await;
    render(&report);

    println!();
    let issues = report.issue_count();
    if issues == 0 {
        println!("All checks passed. Springtale is ready to run.");
    } else {
        println!(
            "{issues} issue{} found. Fix the items above and run `springtale doctor` again.",
            if issues == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

fn render(report: &Report) {
    for check in &report.checks {
        print_check(check);
    }
}

fn print_check(check: &Check) {
    let tag = match check.severity {
        Severity::Ok => "[OK]  ",
        Severity::Warn => "[WARN]",
        Severity::Fail => "[FAIL]",
    };
    println!("{tag} {}", check.label);
    if let Some(detail) = &check.detail {
        println!("       {detail}");
    }
    if let Some(hint) = &check.fix_hint {
        println!("       {hint}");
    }
}
