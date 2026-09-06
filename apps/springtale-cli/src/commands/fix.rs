//! `springtale fix` — thin CLI wrapper around the error-fix operations.
//!
//! The guide table and auto-fix logic live in
//! `springtale_runtime::operations::error_fixes`. This file only renders.

use anyhow::Result;

use springtale_runtime::operations::error_fixes::{self, FixGuide};

use crate::output;
use crate::store::{PassphraseOpts, derive_db_key_hex};

pub async fn run(error_id: &str, opts: &PassphraseOpts, json_out: bool) -> Result<()> {
    let Some(guide) = error_fixes::lookup(error_id) else {
        // Not an error: an unknown id lists the known ones. Both forms go
        // through the same helper so `--json` is machine-readable here too.
        let known = error_fixes::all_guides();
        let body = serde_json::json!({ "error_id": error_id, "known": false, "known_ids": known });
        return output::emit(json_out, &body, |_| {
            render_unknown(error_id, known)
                .trim_end_matches('\n')
                .to_owned()
        });
    };

    // The auto-fix runs before anything is printed, so `--json` gets one
    // object — guide plus outcome — instead of prose interleaved with it.
    let outcome = if guide.has_auto_fix {
        // Fixers that open the store need the key; the user has the
        // passphrase at hand, so ask now instead of reporting "locked".
        let key = derive_db_key_hex(opts)?;
        Some(error_fixes::auto_fix_with_key(guide.id, Some(&key)).await)
    } else {
        None
    };

    let body = serde_json::json!({ "guide": guide, "outcome": outcome });
    output::emit(json_out, &body, |_| {
        let mut out = render_guide(guide);
        if let Some(outcome) = &outcome {
            out.push_str("\nAttempting automated fix...\n\n");
            for msg in &outcome.messages {
                out.push_str(&format!("  {msg}\n"));
            }
            out.push_str(&format!(
                "\nResult: {}",
                if outcome.success {
                    "success"
                } else {
                    "no change"
                }
            ));
        }
        out.trim_end_matches('\n').to_owned()
    })
}

fn render_guide(guide: &FixGuide) -> String {
    let mut out = format!("{}: {}\n\n", guide.id, guide.title);
    if !guide.causes.is_empty() {
        out.push_str("Common causes:\n");
        for cause in guide.causes {
            out.push_str(&format!("  - {cause}\n"));
        }
        out.push('\n');
    }
    if !guide.suggestions.is_empty() {
        out.push_str("Suggestions:\n");
        for suggestion in guide.suggestions {
            out.push_str(&format!("  - {suggestion}\n"));
        }
    }
    out
}

fn render_unknown(error_id: &str, known: &[FixGuide]) -> String {
    let mut out = format!("Unknown error ID: {error_id}\n\nKnown error IDs:\n");
    for guide in known {
        out.push_str(&format!("  {} — {}\n", guide.id, guide.title));
    }
    out
}
