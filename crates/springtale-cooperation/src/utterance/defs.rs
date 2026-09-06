//! The utterance def table — data like a RimWorld `MoteDef`. One row per
//! `UtteranceKind`; `[cooperation.utterances]` in springtale.toml may
//! override any entry.
//!
//! No images: glyphs are text in a subset of Symbols Nerd Font the app
//! ships. No faces (Yuki, Maddux and Masuda 2007), no check or cross for
//! outcome (a tick means *wrong* in ja/ko/fi/sv). Severity is ISO 3864
//! shape plus colour.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::types::{Carrier, Shape, Tone};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct UtteranceDef {
    pub carrier: Carrier,
    pub shape: Shape,
    pub tone: Tone,
    /// Glyph frames for the default locale. One entry = static.
    pub frames: Vec<String>,
    /// Per-locale overrides of `frames`, keyed by locale ("ja", "ar", …).
    #[serde(default)]
    pub locales: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub mirror_rtl: bool,
    /// Key into the frontend locale dictionaries.
    pub label_key: String,
    pub ttl_ticks: u32,
    /// Stardew `blockedIntervalBeforeEmote`, in ticks, per (agent, kind).
    pub block_ticks: u32,
}

/// Def table keyed by `UtteranceKind::name()`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct UtteranceDefs(pub HashMap<String, UtteranceDef>);

// Nerd Font codepoints, verified against ryanoasis/nerd-fonts
// `glyphnames.json` on 2026-09-04. Material Design Icons live in
// F0001–F1AF0 (nf-md-*). Do not substitute without re-verifying.
/// nf-md-sleep — zzz.
pub const MD_SLEEP: &str = "\u{f04b2}";
/// nf-md-heart.
pub const MD_HEART: &str = "\u{f02d1}";
/// nf-md-hand_back_right — open hand, grasp.
pub const MD_HAND_GRAB: &str = "\u{f0e47}";
/// nf-md-bullhorn.
pub const MD_BULLHORN: &str = "\u{f00e6}";
/// nf-md-alert — the ISO warning triangle glyph.
pub const MD_ALERT: &str = "\u{f0026}";
/// nf-md-dots_circle — circles orbiting: Walker's squeans.
pub const MD_SQUEAN: &str = "\u{f1978}";
/// Role glyphs for `yield` (the beneficiary's role), same font, all verified.
pub const MD_ROLE_SCOUT: &str = "\u{f00a5}"; // nf-md-binoculars
pub const MD_ROLE_WORKER: &str = "\u{f08ea}"; // nf-md-hammer
pub const MD_ROLE_GUARD: &str = "\u{f0498}"; // nf-md-shield
pub const MD_ROLE_ANALYST: &str = "\u{f0349}"; // nf-md-magnify
pub const MD_ROLE_SENTINEL: &str = "\u{f0208}"; // nf-md-eye

/// Placeholder frame resolved by the renderer to the beneficiary's role glyph.
pub const ROLE_PLACEHOLDER: &str = "role";

/// Every codepoint constant above, for the font-subset and range checks.
pub const ALL_CODEPOINT_CONSTS: &[&str] = &[
    MD_SLEEP,
    MD_HEART,
    MD_HAND_GRAB,
    MD_BULLHORN,
    MD_ALERT,
    MD_SQUEAN,
    MD_ROLE_SCOUT,
    MD_ROLE_WORKER,
    MD_ROLE_GUARD,
    MD_ROLE_ANALYST,
    MD_ROLE_SENTINEL,
];

#[allow(clippy::too_many_arguments)]
fn def(
    carrier: Carrier,
    shape: Shape,
    tone: Tone,
    frames: &[&str],
    mirror_rtl: bool,
    label_key: &str,
    ttl_ticks: u32,
    block_ticks: u32,
) -> UtteranceDef {
    UtteranceDef {
        carrier,
        shape,
        tone,
        frames: frames.iter().map(|s| (*s).to_owned()).collect(),
        locales: HashMap::new(),
        mirror_rtl,
        label_key: label_key.to_owned(),
        ttl_ticks,
        block_ticks,
    }
}

impl Default for UtteranceDefs {
    fn default() -> Self {
        use Carrier::{Burst, None, Speech, Thought};
        use Shape::{Circle, Square, Triangle};
        use Tone::{Alert, Calm, Urgent};
        Self(HashMap::from([
            // Exclamation is universal; Stardew exclamationEmote = 16.
            (
                "firing".into(),
                def(Burst, Triangle, Alert, &["!"], false, "utter.firing", 2, 3),
            ),
            // The typing indicator; Stardew pauseEmote = 40. Three frames, 250 ms each.
            (
                "working".into(),
                def(
                    Speech,
                    Square,
                    Calm,
                    &["·", "··", "···"],
                    false,
                    "utter.working",
                    2,
                    3,
                ),
            ),
            (
                "listening".into(),
                def(
                    Thought,
                    Square,
                    Calm,
                    &["·"],
                    false,
                    "utter.listening",
                    2,
                    30,
                ),
            ),
            // Sleep glyph reads across cultures; Stardew sleepEmote = 24.
            (
                "idle".into(),
                def(
                    Thought,
                    Square,
                    Calm,
                    &[MD_SLEEP],
                    false,
                    "utter.idle",
                    2,
                    30,
                ),
            ),
            // Q*bert's grawlix (Walker 1980). Never ✓ or ✗: a tick means wrong in ja/ko/fi/sv.
            (
                "failed".into(),
                def(
                    Burst,
                    Circle,
                    Urgent,
                    &["@#!?", "#!?@", "!?@#", "?@#!"],
                    false,
                    "utter.failed",
                    3,
                    1,
                ),
            ),
            // Squeans: circles orbiting the head (Walker 1980). No skull: generational and cultural.
            (
                "down".into(),
                def(
                    None,
                    Circle,
                    Urgent,
                    &[MD_SQUEAN],
                    false,
                    "utter.down",
                    6,
                    0,
                ),
            ),
            (
                "claimed".into(),
                def(
                    Speech,
                    Square,
                    Calm,
                    &[MD_HAND_GRAB],
                    true,
                    "utter.claimed",
                    1,
                    1,
                ),
            ),
            // The Sims: the icon of the thing you're thinking of. "role" resolves to the beneficiary's role glyph.
            (
                "yield".into(),
                def(
                    Thought,
                    Square,
                    Calm,
                    &[ROLE_PLACEHOLDER],
                    false,
                    "utter.yield",
                    2,
                    1,
                ),
            ),
            // Heart survives the Barbieri 2016 and Guntuku 2019 corpora as care. Maintainer ruling 2026-09-04.
            (
                "helping".into(),
                def(
                    Speech,
                    Square,
                    Calm,
                    &[MD_HEART],
                    false,
                    "utter.helping",
                    2,
                    1,
                ),
            ),
            (
                "rally".into(),
                def(
                    Burst,
                    Triangle,
                    Alert,
                    &[MD_BULLHORN, "!!"],
                    true,
                    "utter.rally",
                    3,
                    0,
                ),
            ),
            // The ISO 3864 warning triangle itself.
            (
                "cascade".into(),
                def(
                    Burst,
                    Triangle,
                    Urgent,
                    &[MD_ALERT],
                    false,
                    "utter.cascade",
                    3,
                    0,
                ),
            ),
        ]))
    }
}

impl UtteranceDefs {
    pub fn get(&self, name: &str) -> Option<&UtteranceDef> {
        self.0.get(name)
    }

    /// Def for a kind, by its stable name.
    pub fn for_kind(&self, kind: &super::types::UtteranceKind) -> Option<&UtteranceDef> {
        self.get(kind.name())
    }

    /// Every codepoint the shipped font subset must contain. Fed to
    /// `pyftsubset`. `"role"` is a placeholder resolved by the renderer,
    /// not a glyph string.
    pub fn codepoints(&self) -> BTreeSet<char> {
        self.0
            .values()
            .flat_map(|d| d.frames.iter().chain(d.locales.values().flatten()))
            .filter(|s| s.as_str() != ROLE_PLACEHOLDER)
            .flat_map(|s| s.chars())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadence::AgentId;
    use crate::utterance::types::UtteranceKind;

    fn every_kind() -> Vec<UtteranceKind> {
        let a = AgentId(uuid::Uuid::nil());
        vec![
            UtteranceKind::Firing,
            UtteranceKind::Working,
            UtteranceKind::Listening,
            UtteranceKind::Idle,
            UtteranceKind::Failed,
            UtteranceKind::Down,
            UtteranceKind::Claimed {
                task: uuid::Uuid::nil(),
            },
            UtteranceKind::Yield { beneficiary: a },
            UtteranceKind::Helping { target: a },
            UtteranceKind::Rally,
            UtteranceKind::Cascade { streak: 1 },
        ]
    }

    #[test]
    fn test_default_every_kind_has_def_with_nonempty_frames() {
        let defs = UtteranceDefs::default();
        let kinds = every_kind();
        assert_eq!(defs.0.len(), kinds.len(), "one row per kind");
        for kind in kinds {
            let def = defs
                .for_kind(&kind)
                .unwrap_or_else(|| panic!("no def for {}", kind.name()));
            assert!(!def.frames.is_empty(), "{} has no frames", kind.name());
            assert!(
                def.frames.iter().all(|f| !f.is_empty()),
                "{} has an empty frame",
                kind.name()
            );
            assert_eq!(def.label_key, format!("utter.{}", kind.name()));
        }
    }

    fn in_nerd_font_pua(c: char) -> bool {
        matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFF)
    }

    #[test]
    fn test_codepoint_constants_are_in_nerd_font_private_use_ranges() {
        for s in ALL_CODEPOINT_CONSTS {
            let mut chars = s.chars();
            let c = chars.next().unwrap_or_else(|| panic!("empty constant"));
            assert!(
                chars.next().is_none(),
                "constant {s:?} is more than one codepoint"
            );
            assert!(
                in_nerd_font_pua(c),
                "U+{:04X} outside Nerd Font PUA",
                c as u32
            );
        }
        let defs = UtteranceDefs::default();
        let mut pua = 0;
        for c in defs.codepoints() {
            if !c.is_ascii() && c != '·' {
                assert!(
                    in_nerd_font_pua(c),
                    "U+{:04X} outside Nerd Font PUA",
                    c as u32
                );
                pua += 1;
            }
        }
        assert_eq!(pua, 6, "six distinct icon codepoints in the default table");
        assert!(
            !defs.codepoints().contains(&'r'),
            "role placeholder must not leak into the subset"
        );
    }
}
