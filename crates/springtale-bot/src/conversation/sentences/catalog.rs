//! Sentence templates, one file per locale.
//!
//! hassil's shape, in Rust: a verb is recognised by matching the
//! utterance against sentence templates such as `pause {formation}`,
//! where `{formation}` is a slot filled from the user's own formation
//! names — read at match time, never hard-coded. The templates live in
//! `{locale}.yaml` beside this file, one per language
//! `packages/ui/src/i18n/locales` speaks.
//!
//! Six locales are populated — `en`, `es`, `fr`, `pt`, `tl`, `ar`. `ja`
//! and `th` are deliberately still stubs: the tokenizer segments on
//! spaces and those two scripts are written without them, so templates
//! could not match (each file says so at the top). A locale with no
//! phrases falls back to English.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// The phrases for one verb.
#[derive(Debug, Clone, Deserialize)]
pub struct VerbSentences {
    #[serde(default)]
    pub phrases: Vec<String>,
}

/// One locale's sentence file.
#[derive(Debug, Clone, Deserialize)]
pub struct SentenceCatalog {
    pub locale: String,
    #[serde(default)]
    pub verbs: HashMap<String, VerbSentences>,
}

impl SentenceCatalog {
    /// Phrases for a dotted verb name, falling back to English when this
    /// locale has not been translated yet.
    pub fn phrases(&self, verb: &str) -> &[String] {
        match self.verbs.get(verb) {
            Some(v) if !v.phrases.is_empty() => &v.phrases,
            _ if self.locale != "en" => english().phrases(verb),
            _ => &[],
        }
    }
}

/// Locales shipped with a sentence file — the same eight the UI speaks
/// (`packages/ui/src/i18n/locales`).
pub const LOCALES: &[&str] = &["en", "ar", "es", "fr", "ja", "pt", "th", "tl"];

const EN: &str = include_str!("en.yaml");
const AR: &str = include_str!("ar.yaml");
const ES: &str = include_str!("es.yaml");
const FR: &str = include_str!("fr.yaml");
const JA: &str = include_str!("ja.yaml");
const PT: &str = include_str!("pt.yaml");
const TH: &str = include_str!("th.yaml");
const TL: &str = include_str!("tl.yaml");

fn source(locale: &str) -> &'static str {
    match locale {
        "ar" => AR,
        "es" => ES,
        "fr" => FR,
        "ja" => JA,
        "pt" => PT,
        "th" => TH,
        "tl" => TL,
        _ => EN,
    }
}

/// Parsed catalogues, built once per process.
fn cache() -> &'static HashMap<String, SentenceCatalog> {
    static CACHE: OnceLock<HashMap<String, SentenceCatalog>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        for locale in LOCALES {
            match serde_yaml::from_str::<SentenceCatalog>(source(locale)) {
                Ok(cat) => {
                    map.insert((*locale).to_owned(), cat);
                }
                Err(e) => {
                    tracing::error!(locale = %locale, error = %e, "sentence file failed to parse");
                }
            }
        }
        map
    })
}

/// The English catalogue — the fallback for every untranslated locale.
pub fn english() -> &'static SentenceCatalog {
    static EMPTY: OnceLock<SentenceCatalog> = OnceLock::new();
    cache().get("en").unwrap_or_else(|| {
        EMPTY.get_or_init(|| SentenceCatalog {
            locale: "en".to_owned(),
            verbs: HashMap::new(),
        })
    })
}

/// The catalogue for a locale, English when the locale is unknown.
pub fn for_locale(locale: &str) -> &'static SentenceCatalog {
    cache().get(locale).unwrap_or_else(english)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use springtale_runtime::operations::platform::platform_verbs;

    /// Locales with real sentence templates. `ja` and `th` are stubs
    /// pending a word segmenter — see their files.
    const TRANSLATED: &[&str] = &["en", "es", "fr", "pt", "tl", "ar"];

    #[test]
    fn test_every_locale_file_parses() {
        for locale in LOCALES {
            assert_eq!(for_locale(locale).locale, **locale, "locale {locale}");
        }
    }

    #[test]
    fn test_english_covers_every_platform_verb() {
        for verb in platform_verbs() {
            assert!(
                !english().phrases(verb.name).is_empty(),
                "verb `{}` has no English sentence template",
                verb.name
            );
        }
    }

    /// Every locale that ships phrases ships them for EVERY verb — a
    /// half-translated file would silently answer some verbs in one
    /// language and some in another.
    #[test]
    fn test_translated_locales_cover_every_platform_verb() {
        for locale in TRANSLATED {
            let cat = for_locale(locale);
            for verb in platform_verbs() {
                assert!(
                    cat.verbs
                        .get(verb.name)
                        .is_some_and(|v| !v.phrases.is_empty()),
                    "locale `{locale}` has no sentence template for `{}`",
                    verb.name
                );
            }
        }
    }

    /// A verb's slots must survive translation: a translated phrase may
    /// reorder them, but it may not invent or drop one.
    #[test]
    fn test_translated_phrases_use_declared_slots() {
        for locale in TRANSLATED {
            for verb in platform_verbs() {
                for phrase in for_locale(locale).phrases(verb.name) {
                    for slot in phrase
                        .split('{')
                        .skip(1)
                        .filter_map(|s| s.split('}').next())
                    {
                        assert!(
                            verb.args.contains(&slot)
                                || matches!(slot, "intent" | "key" | "value" | "adapter" | "id"),
                            "locale `{locale}`: `{}` uses unknown slot `{{{slot}}}`",
                            verb.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_stub_locale_falls_back_to_english() {
        // `ja` and `th` are stubs on purpose (no word segmentation).
        assert!(for_locale("ja").verbs.is_empty());
        assert_eq!(
            for_locale("ja").phrases("formation.pause"),
            english().phrases("formation.pause")
        );
    }
}
