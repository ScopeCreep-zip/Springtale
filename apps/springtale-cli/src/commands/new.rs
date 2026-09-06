//! `springtale new` — thin CLI wrapper around the template operations.
//!
//! Template content and write logic live in
//! `springtale_runtime::operations::templates`. The daemon picks the
//! destination directory to prevent path traversal.

use anyhow::{Context, Result};

use springtale_runtime::operations::templates::{self, TemplateError};

use crate::output;

pub fn run(template: &str, json_out: bool) -> Result<()> {
    match templates::write_to(template) {
        Ok(report) => output::emit(json_out, &report, |r| {
            let mut out = format!("Creating {} project in {}\n", r.template, r.dir.display());
            for path in &r.created {
                out.push_str(&format!("  Created {}\n", path.display()));
            }
            out.push_str("\nDone! Next steps:\n");
            out.push_str("  1. Store any API tokens: springtale vault set <name>\n");
            out.push_str("  2. Run: springtale init\n");
            out.push_str("  3. Run: springtale server start");
            out
        }),
        Err(TemplateError::Unknown(_)) => {
            // The listing is the useful part of the failure, so it is
            // emitted (JSON or text) before the non-zero exit.
            let available = templates::list();
            let body = serde_json::json!({
                "unknown_template": template,
                "available": available,
            });
            output::emit(json_out, &body, |_| {
                render_template_list(template, available)
            })?;
            anyhow::bail!("use one of the template names listed above")
        }
        Err(e) => Err(e).context("failed to create project"),
    }
}

fn render_template_list(requested: &str, available: &[templates::Template]) -> String {
    let mut out = format!("Unknown template: {requested}\n\nAvailable templates:\n");
    for t in available {
        out.push_str(&format!("  {:20} — {}\n", t.name, t.description));
    }
    out.trim_end_matches('\n').to_owned()
}
