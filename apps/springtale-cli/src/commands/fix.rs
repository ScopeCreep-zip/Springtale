//! `springtale fix` — thin CLI wrapper around the error-fix operations.
//!
//! The guide table and auto-fix logic live in
//! `springtale_runtime::operations::error_fixes`. This file only renders.

use anyhow::Result;

use springtale_runtime::operations::error_fixes::{self, FixGuide};

pub async fn run(error_id: &str) -> Result<()> {
    let Some(guide) = error_fixes::lookup(error_id) else {
        print_unknown(error_id);
        return Ok(());
    };

    print_guide(guide);

    if guide.has_auto_fix {
        println!("\nAttempting automated fix...\n");
        let outcome = error_fixes::auto_fix(guide.id).await;
        for msg in &outcome.messages {
            println!("  {msg}");
        }
        println!(
            "\nResult: {}",
            if outcome.success {
                "success"
            } else {
                "no change"
            }
        );
    }

    Ok(())
}

fn print_guide(guide: &FixGuide) {
    println!("{}: {}\n", guide.id, guide.title);
    if !guide.causes.is_empty() {
        println!("Common causes:");
        for cause in guide.causes {
            println!("  - {cause}");
        }
        println!();
    }
    if !guide.suggestions.is_empty() {
        println!("Suggestions:");
        for suggestion in guide.suggestions {
            println!("  - {suggestion}");
        }
    }
}

fn print_unknown(error_id: &str) {
    println!("Unknown error ID: {error_id}\n");
    println!("Known error IDs:");
    for guide in error_fixes::all_guides() {
        println!("  {} — {}", guide.id, guide.title);
    }
}
