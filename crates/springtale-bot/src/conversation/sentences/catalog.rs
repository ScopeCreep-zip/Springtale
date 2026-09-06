//! Sentence templates, one file per locale.
//!
//! hassil's shape, in Rust: a verb is recognised by matching the
//! utterance against sentence templates such as `pause {formation}`,
//! where `{formation}` is a slot filled from the user's own formation
//! names — read at match time, never hard-coded. The templates live in
//! `{locale}.yaml` beside this file, one per language
//! `packages/ui/src/i18n/locales` speaks.
//!
//! Only `en` is populated today; the other seven are stubs, and a
//! locale with no phrases falls back to English.

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

/// Locales shipped with a sentence file. `en` is real; the rest are
/// stubs awaiting translation.
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

    #[test]
    fn test_stub_locale_falls_back_to_english() {
        assert_eq!(
            for_locale("fr").phrases("formation.pause"),
            english().phrases("formation.pause")
        );
    }
}
