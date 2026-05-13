//! CSS-selector extraction via `scraper` (html5ever + Servo
//! selectors).
//!
//! Authors declare a map `{ field_name: "css selector" }`. Each
//! selector evaluates against the source HTML; the first match's
//! text content becomes the field value. Suffix conventions:
//!
//! - `selector` — first match's text (string).
//! - `selector :all` — array of every match's text.
//! - `selector @attr` — first match's named attribute.
//!
//! Output is a JSON object with the same keys as the schema.

use scraper::{Html, Selector};
use serde_json::{Map, Value};

use super::{source_as_str, ExtractError};

pub fn extract(
    source: &Value,
    schema: &Map<String, Value>,
) -> Result<Value, ExtractError> {
    let html = source_as_str(source)?;
    let doc = Html::parse_document(html);
    let mut out = Map::with_capacity(schema.len());

    for (field, raw_selector) in schema {
        let selector_str = raw_selector
            .as_str()
            .ok_or_else(|| ExtractError::SchemaFieldType {
                field: field.clone(),
            })?;
        let spec = SelectorSpec::parse(selector_str);
        let parsed = Selector::parse(spec.selector).map_err(|e| {
            ExtractError::CssSelector {
                selector: spec.selector.to_owned(),
                reason: e.to_string(),
            }
        })?;

        let value = match (spec.mode, &spec.attr) {
            (Mode::First, None) => doc
                .select(&parsed)
                .next()
                .map(|el| Value::String(text_of(&el)))
                .unwrap_or(Value::Null),
            (Mode::First, Some(attr)) => doc
                .select(&parsed)
                .next()
                .and_then(|el| el.value().attr(attr).map(|s| Value::String(s.to_owned())))
                .unwrap_or(Value::Null),
            (Mode::All, None) => Value::Array(
                doc.select(&parsed)
                    .map(|el| Value::String(text_of(&el)))
                    .collect(),
            ),
            (Mode::All, Some(attr)) => Value::Array(
                doc.select(&parsed)
                    .filter_map(|el| {
                        el.value().attr(attr).map(|s| Value::String(s.to_owned()))
                    })
                    .collect(),
            ),
        };
        out.insert(field.clone(), value);
    }

    Ok(Value::Object(out))
}

#[derive(Debug)]
struct SelectorSpec<'a> {
    selector: &'a str,
    mode: Mode,
    attr: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    First,
    All,
}

impl<'a> SelectorSpec<'a> {
    fn parse(raw: &'a str) -> Self {
        let raw = raw.trim();

        // `@attr` suffix → attribute mode.
        let (selector_part, attr) = match raw.rsplit_once(" @") {
            Some((sel, attr)) => (sel.trim_end(), Some(attr.trim().to_owned())),
            None => (raw, None),
        };

        // `:all` suffix → multi-match mode.
        let (selector, mode) = match selector_part.strip_suffix(" :all") {
            Some(rest) => (rest.trim_end(), Mode::All),
            None => (selector_part, Mode::First),
        };

        SelectorSpec {
            selector,
            mode,
            attr,
        }
    }
}

fn text_of(el: &scraper::ElementRef<'_>) -> String {
    el.text().collect::<String>().trim().to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE: &str = r#"<!DOCTYPE html><html><body>
<h1 class="title">Hello World</h1>
<a class="link" href="/a">First</a>
<a class="link" href="/b">Second</a>
<a class="link" href="/c">Third</a>
</body></html>"#;

    #[test]
    fn extracts_text_via_first_match() {
        let source = Value::String(SAMPLE.into());
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "title": "h1.title",
        }))
        .unwrap();
        let out = extract(&source, &schema).unwrap();
        assert_eq!(out["title"], "Hello World");
    }

    #[test]
    fn extracts_all_matches_with_all_suffix() {
        let source = Value::String(SAMPLE.into());
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "links": "a.link :all",
        }))
        .unwrap();
        let out = extract(&source, &schema).unwrap();
        assert_eq!(out["links"][0], "First");
        assert_eq!(out["links"][1], "Second");
        assert_eq!(out["links"][2], "Third");
    }

    #[test]
    fn extracts_attribute_with_at_suffix() {
        let source = Value::String(SAMPLE.into());
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "first_href": "a.link @href",
        }))
        .unwrap();
        let out = extract(&source, &schema).unwrap();
        assert_eq!(out["first_href"], "/a");
    }

    #[test]
    fn extracts_all_attributes_with_combined_suffixes() {
        let source = Value::String(SAMPLE.into());
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "all_hrefs": "a.link :all @href",
        }))
        .unwrap();
        let out = extract(&source, &schema).unwrap();
        assert_eq!(out["all_hrefs"][0], "/a");
        assert_eq!(out["all_hrefs"][1], "/b");
        assert_eq!(out["all_hrefs"][2], "/c");
    }

    #[test]
    fn no_match_returns_null() {
        let source = Value::String(SAMPLE.into());
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "missing": "h2.nope",
        }))
        .unwrap();
        let out = extract(&source, &schema).unwrap();
        assert_eq!(out["missing"], Value::Null);
    }

    #[test]
    fn invalid_selector_returns_css_error() {
        let source = Value::String(SAMPLE.into());
        let schema: Map<String, Value> = serde_json::from_value(json!({
            "bad": "::: not a selector",
        }))
        .unwrap();
        let err = extract(&source, &schema).unwrap_err();
        assert!(matches!(err, ExtractError::CssSelector { .. }));
    }
}
