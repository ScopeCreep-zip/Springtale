//! `springtale new` — thin CLI wrapper around the template operations.
//!
//! Template content and write logic live in
//! `springtale_runtime::operations::templates`. This file only parses
//! args and renders the result.

use std::path::Path;

use anyhow::{Context, Result};

use springtale_runtime::operations::templates::{self, TemplateError};

pub fn run(template: &str, dir: &Path) -> Result<()> {
    match templates::write_to(template, dir) {
        Ok(report) => {
            println!(
                "Creating {} project in {}",
                report.template,
                report.dir.display()
            );
            for path in &report.created {
                println!("  Created {}", path.display());
            }
            println!("\nDone! Next steps:");
            println!("  1. Store any API tokens: springtale vault set <name>");
            println!("  2. Run: springtale init");
            println!("  3. Run: springtale server start");
            Ok(())
        }
        Err(TemplateError::Unknown(_)) => {
            print_template_list(template);
            anyhow::bail!("use one of the template names listed above")
        }
        Err(e) => Err(e).context("failed to create project"),
    }
}

fn print_template_list(requested: &str) {
    println!("Unknown template: {requested}");
    println!("\nAvailable templates:");
    for t in templates::list() {
        println!("  {:20} — {}", t.name, t.description);
    }
}
