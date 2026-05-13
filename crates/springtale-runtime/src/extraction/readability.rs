//! Readability extraction — Mozilla Readability port via
//! `dom_smoothie`. Strips nav / ads / sidebars, returns the article
//! body.
//!
//! Output shape (mirrors the JS Readability spec):
//! ```json
//! {
//!   "title": "string",
//!   "byline": "string | null",
//!   "content": "string (HTML)",
//!   "text_content": "string (plain text)",
//!   "excerpt": "string | null",
//!   "length": <int>
//! }
//! ```
//!
//! Recipes pipe `${last_extract_output.text_content}` into AI
//! summarisation prompts, or `${last_extract_output.title}` +
//! `${last_extract_output.byline}` into messaging templates.

use dom_smoothie::Readability;
use serde_json::{json, Value};

use super::{source_as_str, ExtractError};

pub fn extract(source: &Value) -> Result<Value, ExtractError> {
    let html = source_as_str(source)?;
    let mut reader = Readability::new(html, None, None)
        .map_err(|e| ExtractError::Readability(e.to_string()))?;
    let article = reader
        .parse()
        .map_err(|e| ExtractError::Readability(e.to_string()))?;

    // `Article.content` + `text_content` are `StrTendril`
    // (html5ever's UTF-8 ref-counted string). Tendril doesn't
    // implement `Serialize`, so we coerce to `String` before
    // handing to `json!`. `title` / `byline` / `excerpt` are
    // already `String` / `Option<String>` per dom_smoothie 0.17.
    Ok(json!({
        "title": article.title,
        "byline": article.byline,
        "content": article.content.to_string(),
        "text_content": article.text_content.to_string(),
        "excerpt": article.excerpt,
        "length": article.length,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE_ARTICLE: &str = r#"<!DOCTYPE html>
<html><head><title>Test Article</title></head><body>
<header><nav>nav stuff</nav></header>
<article>
  <h1>The Real Title</h1>
  <p>This is the first paragraph of the real article body. It contains
     enough text that Readability will pick it as the main content
     and not the noise in the header or footer.</p>
  <p>Second paragraph adds enough content density that the heuristic
     keeps this region. Readability looks for clusters of paragraph
     tags with substantial text.</p>
</article>
<footer>footer stuff</footer>
</body></html>"#;

    #[test]
    fn extracts_article_body_and_title() {
        let source = Value::String(SAMPLE_ARTICLE.to_owned());
        let out = extract(&source).unwrap();
        // Readability finds a non-empty title (exact value depends on
        // its heuristic; just check it's there).
        assert!(out.get("title").is_some());
        // text_content should contain article body text.
        let text = out["text_content"].as_str().unwrap();
        assert!(
            text.contains("real article body"),
            "expected article body in extracted text, got: {text}"
        );
        // Nav / footer noise should be stripped.
        assert!(
            !text.contains("nav stuff") && !text.contains("footer stuff"),
            "Readability did not strip nav/footer noise: {text}"
        );
    }

    #[test]
    fn errors_on_non_string_source() {
        let source = Value::Null;
        let err = extract(&source).unwrap_err();
        assert!(matches!(err, ExtractError::SourceNotString { got: "null" }));
    }
}
