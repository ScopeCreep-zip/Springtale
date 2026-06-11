//! RSS / Atom / JSON Feed extraction via `feed-rs`.
//!
//! Output shape:
//! ```json
//! {
//!   "title": "feed title | null",
//!   "entries": [
//!     {
//!       "id": "entry id",
//!       "title": "entry title | null",
//!       "url": "first link href | null",
//!       "summary": "summary text | null",
//!       "published": "RFC 3339 | null"
//!     }
//!   ]
//! }
//! ```
//!
//! Recipes use `${last_extract_output.entries.0.id}` as a dedupe key
//! for the "alert me on new posts" pattern.

use feed_rs::parser;
use serde_json::{Value, json};

use super::{ExtractError, source_as_str};

pub fn extract(source: &Value) -> Result<Value, ExtractError> {
    let body = source_as_str(source)?;
    let feed = parser::parse(body.as_bytes()).map_err(|e| ExtractError::Feed(e.to_string()))?;

    let entries: Vec<Value> = feed
        .entries
        .iter()
        .map(|entry| {
            let id = entry.id.clone();
            let title = entry.title.as_ref().map(|t| t.content.clone());
            let url = entry.links.first().map(|l| l.href.clone());
            let summary = entry
                .summary
                .as_ref()
                .map(|s| s.content.clone())
                .or_else(|| entry.content.as_ref().and_then(|c| c.body.clone()));
            let published = entry.published.or(entry.updated).map(|dt| dt.to_rfc3339());
            json!({
                "id": id,
                "title": title,
                "url": url,
                "summary": summary,
                "published": published,
            })
        })
        .collect();

    let feed_title = feed.title.as_ref().map(|t| t.content.clone());

    Ok(json!({
        "title": feed_title,
        "entries": entries,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE_RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0">
<channel>
  <title>Test Feed</title>
  <link>https://example.com/</link>
  <description>A sample feed.</description>
  <item>
    <title>First post</title>
    <link>https://example.com/1</link>
    <guid>https://example.com/1</guid>
    <description>First summary</description>
    <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
  </item>
  <item>
    <title>Second post</title>
    <link>https://example.com/2</link>
    <guid>https://example.com/2</guid>
    <description>Second summary</description>
    <pubDate>Tue, 02 Jan 2024 00:00:00 GMT</pubDate>
  </item>
</channel>
</rss>"#;

    #[test]
    fn parses_rss_into_entries() {
        let source = Value::String(SAMPLE_RSS.into());
        let out = extract(&source).unwrap();
        assert_eq!(out["title"], "Test Feed");
        let entries = out["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["title"], "First post");
        assert_eq!(entries[0]["url"], "https://example.com/1");
        assert_eq!(entries[0]["summary"], "First summary");
        assert!(entries[0]["id"].as_str().unwrap().contains("example.com/1"));
        assert!(entries[0]["published"].as_str().unwrap().contains("2024"));
    }

    #[test]
    fn errors_on_non_feed_input() {
        let source = Value::String("not a feed".into());
        let err = extract(&source).unwrap_err();
        assert!(matches!(err, ExtractError::Feed(_)));
    }

    #[test]
    fn errors_on_non_string_source() {
        let source = Value::Null;
        let err = extract(&source).unwrap_err();
        assert!(matches!(err, ExtractError::SourceNotString { got: "null" }));
    }
}
