//! Page-diff helpers — produce a stable content hash for the
//! "did this page change" pattern. Used by the `page-change-watcher`
//! universal recipe (Phase A) and surfaces as
//! `${last_extract_output.hash}` for downstream dedupe keys.
//!
//! Pipeline:
//!   1. If the source looks like HTML, strip every tag with an
//!      ammonia Builder configured to (a) drop `<script>` / `<style>`
//!      content entirely so analytics or CSS edits don't flap the
//!      hash and (b) allow zero tags so the markup itself disappears
//!      — what survives is the user-visible text only.
//!   2. Collapse consecutive whitespace to a single space.
//!   3. `blake3` over the normalised bytes.
//!
//! Plain-text inputs skip step 1 but still get whitespace collapsed.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::{source_as_str, ExtractError};

/// Produce `{ hash: "<blake3 hex>", normalised_len: N }` for the
/// source. Recipe authors feed this output into an
/// [`super::super::ExtractKind::Passthrough`] dedupe step keyed on
/// `${last_extract_output.hash}`.
pub fn hash(source: &Value) -> Result<Value, ExtractError> {
    let body = source_as_str(source)?;
    let text = if looks_like_html(body) {
        text_from_html(body)
    } else {
        body.to_owned()
    };
    let normalised = collapse_whitespace(&text);
    let hash = blake3::hash(normalised.as_bytes());
    Ok(json!({
        "hash": hash.to_hex().to_string(),
        "normalised_len": normalised.len(),
    }))
}

/// Strip every HTML tag from `body`, dropping `<script>` and
/// `<style>` content entirely. Returns just the user-visible text.
///
/// ammonia configuration:
///   - `tags(HashSet::new())` — no tag is allowed, so every tag
///     gets unwrapped (its children survive as text).
///   - `clean_content_tags({script, style, noscript, template})` —
///     these tags' children are removed entirely. Without this,
///     `<script>track('a')</script>` and
///     `<script>track('b')</script>` would produce different hashes
///     even though the page content is identical.
fn text_from_html(body: &str) -> String {
    use std::sync::OnceLock;
    static CLEANER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    let cleaner = CLEANER.get_or_init(|| {
        let mut b = ammonia::Builder::new();
        b.tags(HashSet::new());
        let mut drop_content = HashSet::new();
        drop_content.insert("script");
        drop_content.insert("style");
        drop_content.insert("noscript");
        drop_content.insert("template");
        b.clean_content_tags(drop_content);
        b
    });
    cleaner.clean(body).to_string()
}

fn looks_like_html(body: &str) -> bool {
    // Cheap heuristic — sanitise anything that smells like markup.
    body.contains('<') && body.contains('>')
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn same_content_produces_same_hash() {
        let a = Value::String("<p>hello world</p>".into());
        let b = Value::String("<p>hello world</p>".into());
        let h_a = hash(&a).unwrap();
        let h_b = hash(&b).unwrap();
        assert_eq!(h_a["hash"], h_b["hash"]);
    }

    #[test]
    fn whitespace_changes_do_not_flap_hash() {
        let a = Value::String("<p>hello world</p>".into());
        let b = Value::String("<p>\n  hello\n  world\n</p>".into());
        let h_a = hash(&a).unwrap();
        let h_b = hash(&b).unwrap();
        assert_eq!(h_a["hash"], h_b["hash"]);
    }

    #[test]
    fn script_tags_are_stripped_before_hashing() {
        // Two pages that differ only in their analytics script should
        // hash to the same value.
        let with_script = Value::String(
            "<p>real content</p><script>track('a')</script>".into(),
        );
        let without_script = Value::String(
            "<p>real content</p><script>track('b')</script>".into(),
        );
        let h_a = hash(&with_script).unwrap();
        let h_b = hash(&without_script).unwrap();
        assert_eq!(h_a["hash"], h_b["hash"]);
    }

    #[test]
    fn different_body_produces_different_hash() {
        let a = Value::String("<p>hello</p>".into());
        let b = Value::String("<p>world</p>".into());
        let h_a = hash(&a).unwrap();
        let h_b = hash(&b).unwrap();
        assert_ne!(h_a["hash"], h_b["hash"]);
    }

    #[test]
    fn plain_text_input_works() {
        let a = Value::String("just some text".into());
        let h = hash(&a).unwrap();
        assert!(h["hash"].as_str().unwrap().len() == 64); // blake3 hex
    }
}
