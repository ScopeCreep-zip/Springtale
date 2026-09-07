//! `springtale cooperation` — inspect the cooperation primitives.
//!
//! `glyphs` prints every codepoint the utterance def table renders, one
//! `U+XXXX` per line — the `--unicodes-file` input `pyftsubset` consumes in
//! `scripts/build-symbol-font.sh`. With `--check <glyphnames.json>` it first
//! asserts every named Nerd Font constant in `utterance::defs` still maps to
//! the upstream codepoint, and that every private-use codepoint in the table
//! exists upstream at all, so a font rebuild can never ship a tofu box.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use springtale_cooperation::utterance::UtteranceDefs;
use springtale_cooperation::utterance::defs::{ALL_CODEPOINT_CONSTS, NAMED_CODEPOINTS};

use crate::cli::CooperationAction;
use crate::client::Client;
use crate::output;

/// Nerd Fonts' Material Design Icons block, `F0001–F1AF0`.
const PUA_START: u32 = 0xE000;

/// Every codepoint the renderer can draw: the def table's frames and locale
/// overrides plus the role glyphs `yield` resolves at render time.
fn all_codepoints() -> BTreeSet<char> {
    let mut cps = UtteranceDefs::default().codepoints();
    cps.extend(ALL_CODEPOINT_CONSTS.iter().flat_map(|s| s.chars()));
    cps
}

pub async fn utterances(action: CooperationAction, json_out: bool) -> Result<()> {
    let client = Client::from_config()?;
    let body: serde_json::Value = match &action {
        CooperationAction::Utterances => client.get("/cooperation/utterances").await?,
        CooperationAction::Recent { limit } => {
            client
                .get(&format!("{UTTERANCES_RECENT}?limit={limit}"))
                .await?
        }
        CooperationAction::Glyphs { .. } => unreachable!("glyphs is local"),
    };
    output::emit(json_out, &body, |v| {
        serde_json::to_string_pretty(v).unwrap_or_default()
    })
}

/// The recent-utterance feed. The limit is appended as a query.
const UTTERANCES_RECENT: &str = "/cooperation/utterances/recent";

pub fn glyphs(check: Option<&Path>, json_out: bool) -> Result<()> {
    let cps = all_codepoints();
    if let Some(path) = check {
        check_against(path, &cps)?;
    }
    // The plain listing is `pyftsubset --unicodes-file` input, so the
    // human form stays one bare `U+XXXX` per line; `--json` wraps the
    // same list in an envelope for anything that wants to parse it.
    let listed = codepoint_labels(&cps);
    let body = glyphs_body(&listed);
    output::emit(json_out, &body, |_| listed.join("\n"))
}

/// `glyphnames.json` is `{ "METADATA": {...}, "<set>-<name>": { "char", "code" }, ... }`.
fn check_against(path: &Path, cps: &BTreeSet<char>) -> Result<()> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let table: HashMap<String, serde_json::Value> =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    let version = table
        .get("METADATA")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let upstream: HashMap<u32, &str> = table
        .iter()
        .filter(|(k, _)| k.as_str() != "METADATA")
        .filter_map(|(k, v)| {
            let code = v.get("code")?.as_str()?;
            Some((u32::from_str_radix(code, 16).ok()?, k.as_str()))
        })
        .collect();

    let mut problems = Vec::new();
    for (name, glyph) in NAMED_CODEPOINTS {
        let Some(c) = glyph.chars().next() else {
            problems.push(format!("nf-{name}: empty constant"));
            continue;
        };
        let want = u32::from(c);
        let got = table
            .get(*name)
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .and_then(|c| u32::from_str_radix(c, 16).ok());
        match got {
            Some(code) if code == want => eprintln!("ok   nf-{name} U+{want:04X}"),
            Some(code) => problems.push(format!(
                "nf-{name}: defs.rs has U+{want:04X}, upstream {version} has U+{code:04X}"
            )),
            None => problems.push(format!("nf-{name}: not in upstream {version}")),
        }
    }
    for c in cps
        .iter()
        .map(|c| u32::from(*c))
        .filter(|c| *c >= PUA_START)
    {
        if !upstream.contains_key(&c) {
            problems.push(format!("U+{c:04X}: no glyph in upstream {version}"));
        }
    }
    if problems.is_empty() {
        eprintln!(
            "glyphs: {} named, {} total codepoints checked against glyphnames.json {version}",
            NAMED_CODEPOINTS.len(),
            cps.len()
        );
        Ok(())
    } else {
        Err(anyhow!("glyph check failed:\n  {}", problems.join("\n  ")))
    }
}

/// `U+XXXX` labels for every codepoint, in codepoint order.
fn codepoint_labels(cps: &BTreeSet<char>) -> Vec<String> {
    cps.iter()
        .map(|c| format!("U+{:04X}", u32::from(*c)))
        .collect()
}

/// The `cooperation glyphs` body — the same list the human form prints
/// one per line, wrapped so it can be parsed.
fn glyphs_body(listed: &[String]) -> serde_json::Value {
    serde_json::json!({ "codepoints": listed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_glyphs_json_shape_is_a_codepoints_array_of_strings() {
        let listed = codepoint_labels(&all_codepoints());
        let out = json_value(&glyphs_body(&listed));
        assert_eq!(key_set(&out), ["codepoints"]);
        assert!(out["codepoints"].is_array());
        let items = crate::output::array(&out, "codepoints");
        assert!(!items.is_empty(), "the def table renders no glyphs");
        for item in items {
            let label = item.as_str().expect("codepoints are strings");
            assert!(label.starts_with("U+"), "not a codepoint label: {label}");
            assert!(
                u32::from_str_radix(&label[2..], 16).is_ok(),
                "not hex: {label}"
            );
        }
    }

    #[test]
    fn test_glyphs_json_lists_the_same_codepoints_the_human_form_prints() {
        let cps = all_codepoints();
        let listed = codepoint_labels(&cps);
        assert_eq!(listed.len(), cps.len());
        let out = json_value(&glyphs_body(&listed));
        assert_eq!(crate::output::array(&out, "codepoints").len(), cps.len());
    }

    #[test]
    fn test_codepoint_labels_are_four_digit_uppercase_hex() {
        let mut cps = BTreeSet::new();
        cps.insert('\u{e0b0}');
        cps.insert('A');
        assert_eq!(codepoint_labels(&cps), ["U+0041", "U+E0B0"]);
    }
}
