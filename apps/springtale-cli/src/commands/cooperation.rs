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

pub fn glyphs(check: Option<&Path>, json_out: bool) -> Result<()> {
    let cps = all_codepoints();
    if let Some(path) = check {
        check_against(path, &cps)?;
    }
    // The plain listing is `pyftsubset --unicodes-file` input, so the
    // human form stays one bare `U+XXXX` per line; `--json` wraps the
    // same list in an envelope for anything that wants to parse it.
    let listed: Vec<String> = cps
        .iter()
        .map(|c| format!("U+{:04X}", u32::from(*c)))
        .collect();
    let body = serde_json::json!({ "codepoints": &listed });
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
