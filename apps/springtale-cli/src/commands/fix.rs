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
        let body = unknown_body(error_id, known);
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

    let body = fix_body(guide, outcome.as_ref());
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

/// The `fix` body for an unknown error id — not an error, a listing.
fn unknown_body(error_id: &str, known: &[FixGuide]) -> serde_json::Value {
    serde_json::json!({ "error_id": error_id, "known": false, "known_ids": known })
}

/// The `fix` body for a known error id: the guide, plus the outcome of
/// the automated repair when one was attempted.
fn fix_body(guide: &FixGuide, outcome: Option<&error_fixes::FixOutcome>) -> serde_json::Value {
    serde_json::json!({ "guide": guide, "outcome": outcome })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_fix_unknown_id_json_shape_lists_the_known_guides() {
        let known = error_fixes::all_guides();
        let out = json_value(&unknown_body("E999", known));
        assert_eq!(key_set(&out), ["error_id", "known", "known_ids"]);
        assert_eq!(out["error_id"], "E999");
        assert!(out["known"].is_boolean());
        assert_eq!(out["known"], false);
        assert!(out["known_ids"].is_array());
        let listed = crate::output::array(&out, "known_ids");
        assert_eq!(listed.len(), known.len());
        assert!(listed[0]["id"].is_string());
        assert!(listed[0]["title"].is_string());
    }

    #[test]
    fn test_fix_known_id_json_shape_is_guide_plus_outcome() {
        let guide = error_fixes::all_guides().first().expect("a guide exists");
        let out = json_value(&fix_body(guide, None));
        assert_eq!(key_set(&out), ["guide", "outcome"]);
        assert!(out["outcome"].is_null(), "no auto-fix means a null outcome");
        let rendered = &out["guide"];
        assert!(rendered["id"].is_string());
        assert!(rendered["title"].is_string());
        assert!(rendered["causes"].is_array());
        assert!(rendered["suggestions"].is_array());
        assert!(rendered["has_auto_fix"].is_boolean());
    }

    #[test]
    fn test_fix_outcome_json_shape_reports_id_success_and_messages() {
        let guide = error_fixes::all_guides().first().expect("a guide exists");
        let outcome = error_fixes::FixOutcome {
            id: guide.id,
            success: true,
            messages: vec!["recreated springtale.toml".to_owned()],
        };
        let out = json_value(&fix_body(guide, Some(&outcome)));
        let rendered = &out["outcome"];
        assert_eq!(key_set(rendered), ["id", "messages", "success"]);
        assert!(rendered["id"].is_string());
        assert!(rendered["success"].is_boolean());
        assert!(rendered["messages"].is_array());
        assert_eq!(rendered["messages"][0], "recreated springtale.toml");
    }
}
