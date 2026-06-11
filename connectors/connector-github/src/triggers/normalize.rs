//! Anti-corruption normalization for GitHub webhook events.
//!
//! Raw GitHub webhook payloads are deeply nested (`pull_request.title`,
//! `pusher.name`, `repository.full_name`, a `commits[]` array, …). The
//! connector's declared trigger schemas (`super`) are FLAT
//! (`title`, `pusher`, `repository` = "owner/repo", `commits_count`),
//! and recipes consume those flat fields via `${trigger.*}`. This module
//! is the boundary that maps raw → flat, so `${trigger.title}` resolves
//! to the PR title instead of leaving a literal placeholder and
//! `${trigger.pusher}` yields a username instead of a raw `{name,email}`
//! blob.
//!
//! Mapping is defensive and trigger-agnostic: each flat field is pulled
//! from wherever it lives across GitHub's event families
//! (push / pull_request / issues / issue_comment / release), and a field
//! whose source is absent for a given event is simply omitted.

use serde_json::{Map, Value};

/// Map a raw GitHub webhook payload into the connector's flat trigger
/// schema. See module docs.
pub fn normalize(raw: &Value) -> Value {
    let mut out = Map::new();

    insert_if(&mut out, "action", path(raw, &["action"]));
    insert_if(&mut out, "ref", path(raw, &["ref"]));
    insert_if(
        &mut out,
        "repository",
        path(raw, &["repository", "full_name"]),
    );
    insert_if(
        &mut out,
        "number",
        first(
            raw,
            &[
                &["number"],
                &["issue", "number"],
                &["pull_request", "number"],
            ],
        ),
    );
    insert_if(
        &mut out,
        "issue_number",
        first(raw, &[&["issue", "number"], &["number"]]),
    );
    insert_if(
        &mut out,
        "title",
        first(raw, &[&["pull_request", "title"], &["issue", "title"]]),
    );
    insert_if(
        &mut out,
        "body",
        first(
            raw,
            &[
                &["comment", "body"],
                &["pull_request", "body"],
                &["issue", "body"],
            ],
        ),
    );
    insert_if(
        &mut out,
        "author",
        first(
            raw,
            &[
                &["comment", "user", "login"],
                &["pull_request", "user", "login"],
                &["issue", "user", "login"],
                &["sender", "login"],
            ],
        ),
    );
    insert_if(
        &mut out,
        "url",
        first(
            raw,
            &[
                &["pull_request", "html_url"],
                &["issue", "html_url"],
                &["release", "html_url"],
            ],
        ),
    );
    insert_if(
        &mut out,
        "html_url",
        first(
            raw,
            &[
                &["release", "html_url"],
                &["pull_request", "html_url"],
                &["issue", "html_url"],
            ],
        ),
    );
    insert_if(&mut out, "pusher", path(raw, &["pusher", "name"]));
    insert_if(&mut out, "tag_name", path(raw, &["release", "tag_name"]));
    // `commits_count` is the length of the raw `commits[]` array.
    if let Some(Value::Array(commits)) = path_ref(raw, &["commits"]) {
        out.insert("commits_count".to_owned(), Value::from(commits.len()));
    }

    Value::Object(out)
}

fn path_ref<'a>(v: &'a Value, parts: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for p in parts {
        cur = cur.get(p)?;
    }
    Some(cur)
}

fn path(v: &Value, parts: &[&str]) -> Option<Value> {
    path_ref(v, parts).cloned()
}

fn first(v: &Value, paths: &[&[&str]]) -> Option<Value> {
    paths.iter().find_map(|p| path_ref(v, p).cloned())
}

fn insert_if(out: &mut Map<String, Value>, key: &str, val: Option<Value>) {
    match val {
        Some(v) if !v.is_null() => {
            out.insert(key.to_owned(), v);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Real GitHub `push` webhook shape (abbreviated to the fields
    // recipes consume — structure matches the live provider payload).
    #[test]
    fn normalizes_push_event() {
        let raw = json!({
            "ref": "refs/heads/main",
            "repository": { "full_name": "octocat/Hello-World", "id": 1296269 },
            "pusher": { "name": "octocat", "email": "octocat@github.com" },
            "commits": [
                { "id": "a1", "message": "one" },
                { "id": "b2", "message": "two" }
            ]
        });
        let flat = normalize(&raw);
        assert_eq!(flat["ref"], "refs/heads/main");
        assert_eq!(flat["repository"], "octocat/Hello-World");
        assert_eq!(flat["pusher"], "octocat"); // NAME, not the nested object
        assert_eq!(flat["commits_count"], 2); // array length, a real integer
    }

    #[test]
    fn normalizes_pull_request_event() {
        let raw = json!({
            "action": "opened",
            "number": 42,
            "pull_request": {
                "title": "Add the thing",
                "body": "This PR adds the thing.",
                "html_url": "https://github.com/octocat/Hello-World/pull/42",
                "user": { "login": "contributor" }
            },
            "repository": { "full_name": "octocat/Hello-World" }
        });
        let flat = normalize(&raw);
        assert_eq!(flat["action"], "opened");
        assert_eq!(flat["number"], 42);
        assert_eq!(flat["title"], "Add the thing");
        assert_eq!(flat["body"], "This PR adds the thing.");
        assert_eq!(flat["author"], "contributor");
        assert_eq!(
            flat["url"],
            "https://github.com/octocat/Hello-World/pull/42"
        );
        assert_eq!(flat["repository"], "octocat/Hello-World");
    }

    #[test]
    fn normalizes_issues_event() {
        let raw = json!({
            "action": "opened",
            "issue": {
                "number": 7,
                "title": "Bug: thing is broken",
                "body": "Steps to reproduce…",
                "html_url": "https://github.com/octocat/Hello-World/issues/7",
                "user": { "login": "reporter" }
            },
            "repository": { "full_name": "octocat/Hello-World" }
        });
        let flat = normalize(&raw);
        assert_eq!(flat["number"], 7);
        assert_eq!(flat["title"], "Bug: thing is broken");
        assert_eq!(flat["author"], "reporter");
        assert_eq!(
            flat["url"],
            "https://github.com/octocat/Hello-World/issues/7"
        );
    }

    #[test]
    fn normalizes_issue_comment_event() {
        let raw = json!({
            "action": "created",
            "issue": { "number": 11 },
            "comment": {
                "body": "@octocat take a look",
                "user": { "login": "mentioner" }
            },
            "repository": { "full_name": "octocat/Hello-World" }
        });
        let flat = normalize(&raw);
        assert_eq!(flat["issue_number"], 11);
        assert_eq!(flat["body"], "@octocat take a look");
        assert_eq!(flat["author"], "mentioner");
    }

    #[test]
    fn normalizes_release_event() {
        let raw = json!({
            "action": "published",
            "release": {
                "tag_name": "v1.2.0",
                "html_url": "https://github.com/octocat/Hello-World/releases/tag/v1.2.0"
            },
            "repository": { "full_name": "octocat/Hello-World" }
        });
        let flat = normalize(&raw);
        assert_eq!(flat["action"], "published");
        assert_eq!(flat["tag_name"], "v1.2.0");
        assert_eq!(
            flat["html_url"],
            "https://github.com/octocat/Hello-World/releases/tag/v1.2.0"
        );
    }

    #[test]
    fn omits_absent_fields_no_raw_blob() {
        // A minimal payload must not invent fields or leak nested objects.
        let raw = json!({ "ref": "refs/heads/dev", "repository": { "full_name": "a/b" } });
        let flat = normalize(&raw);
        assert_eq!(flat["ref"], "refs/heads/dev");
        assert_eq!(flat["repository"], "a/b");
        assert!(flat.get("title").is_none());
        assert!(flat.get("pusher").is_none());
        assert!(flat.get("commits_count").is_none());
        // No nested objects survive into the flat schema.
        for (_, v) in flat.as_object().unwrap() {
            assert!(
                !v.is_object() && !v.is_array(),
                "flat field leaked a nested value: {v}"
            );
        }
    }
}
