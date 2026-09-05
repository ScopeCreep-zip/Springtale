//! `springtale fix` — thin CLI wrapper around the error-fix operations.
//!
//! The guide table and auto-fix logic live in
//! `springtale_runtime::operations::error_fixes`. This file only renders.

use anyhow::Result;

use springtale_runtime::operations::error_fixes::{self, FixGuide};

use crate::store::{PassphraseOpts, derive_db_key_hex};

pub async fn run(error_id: &str, opts: &PassphraseOpts) -> Result<()> {
    let Some(guide) = error_fixes::lookup(error_id) else {
        print_unknown(error_id);
        return Ok(());
    };

    print_guide(guide);

    if guide.has_auto_fix {
        // Fixers that open the store need the key; the user has the
        // passphrase at hand, so ask now instead of reporting "locked".
        let key = derive_db_key_hex(opts)?;
        println!("\nAttempting automated fix...\n");
        let outcome = error_fixes::auto_fix_with_key(guide.id, Some(&key)).await;
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
