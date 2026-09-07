use anyhow::Result;
use serde::Serialize;

/// Render `data` exactly as `--json` prints it.
///
/// Split out from [`print_json`] so the shape a subcommand emits can be
/// asserted without a terminal (and without a daemon): every `--json`
/// body on stdout is this function's output.
pub fn render_json<T: Serialize>(data: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(data)?)
}

/// Print data as formatted JSON to stdout.
pub fn print_json<T: Serialize>(data: &T) -> Result<()> {
    let json = render_json(data)?;
    println!("{json}");
    Ok(())
}

/// The single `--json` switch every subcommand honours.
///
/// With `--json`, print `data` as pretty JSON. Without it, render the
/// human table via `table` and print it (an empty string prints
/// nothing, so callers can render "nothing to show" themselves).
pub fn emit<T: Serialize>(json: bool, data: &T, table: impl FnOnce(&T) -> String) -> Result<()> {
    if json {
        return print_json(data);
    }
    let rendered = table(data);
    if !rendered.is_empty() {
        println!("{rendered}");
    }
    Ok(())
}

/// The stderr sibling of [`emit`], for commands whose human output is a
/// progress notice rather than data (`travel prepare`, `panic`, `vault
/// duress-setup`, …). The notice keeps going to stderr so stdout stays
/// clean for piping; `--json` still gets a machine-readable object on
/// stdout. The `--json` branch itself lives in [`emit`] and nowhere else.
pub fn emit_status<T: Serialize>(
    json: bool,
    data: &T,
    notice: impl FnOnce(&T) -> String,
) -> Result<()> {
    emit(json, data, |value| {
        let text = notice(value);
        if !text.is_empty() {
            eprintln!("{text}");
        }
        // Nothing for stdout: `emit` prints only a non-empty return.
        String::new()
    })
}

/// Render a table from string cells. Used by every daemon-backed
/// subcommand, which sees JSON rather than typed rows.
pub fn rows_table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut builder = tabled::builder::Builder::default();
    builder.push_record(headers.iter().map(|h| (*h).to_owned()));
    for row in rows {
        builder.push_record(row);
    }
    builder.build().to_string()
}

/// Pull a named array out of a JSON envelope like `{"rules": [...]}`.
pub fn array<'a>(value: &'a serde_json::Value, key: &str) -> &'a [serde_json::Value] {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

/// Render one JSON field as a table cell — strings unquoted, everything
/// else compact JSON, missing as empty.
pub fn cell(value: &serde_json::Value, key: &str) -> String {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// Test-only: the `--json` body a subcommand emits, parsed back into a
/// [`serde_json::Value`] so a test can assert its shape. Goes through
/// [`render_json`] — the same function `--json` prints with — so a test
/// asserts the real output path, not a re-implementation of it.
#[cfg(test)]
pub fn json_value<T: Serialize>(data: &T) -> serde_json::Value {
    serde_json::from_str(&render_json(data).expect("render --json body"))
        .expect("--json output must be valid JSON")
}

/// Test-only: the sorted top-level key set of a JSON object.
#[cfg(test)]
pub fn key_set(value: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use serde_json::json;

    /// The pretty-print family (`bot status`, `formation get`, `recipe
    /// get`, `config list`, `drift`, `safety get`, `canvas`, `rule
    /// schema`, …) hands the daemon document straight to `--json`. The
    /// contract is that nothing is dropped, renamed, or retyped on the
    /// way through.
    #[test]
    fn test_render_json_passes_a_daemon_document_through_unchanged() {
        let doc = json!({
            "status": "running",
            "uptime_secs": 91,
            "degraded": false,
            "adapter": null,
            "formations": [{ "id": "f-1", "members": ["telegram", "github"] }],
        });
        assert_eq!(json_value(&doc), doc);
    }

    #[test]
    fn test_emit_with_json_never_runs_the_table_renderer() {
        let called = Cell::new(false);
        emit(true, &json!({ "ok": true }), |_| {
            called.set(true);
            String::new()
        })
        .expect("emit");
        assert!(!called.get(), "--json must not render the human table");
    }

    #[test]
    fn test_emit_without_json_runs_the_table_renderer() {
        let called = Cell::new(false);
        emit(false, &json!({ "ok": true }), |_| {
            called.set(true);
            String::new()
        })
        .expect("emit");
        assert!(called.get());
    }

    #[test]
    fn test_emit_status_with_json_never_runs_the_notice() {
        let called = Cell::new(false);
        emit_status(true, &json!({ "wiped": true }), |_| {
            called.set(true);
            String::new()
        })
        .expect("emit_status");
        assert!(!called.get(), "--json must not render the stderr notice");
    }

    #[test]
    fn test_array_missing_or_non_array_key_is_empty() {
        let v = json!({ "rules": [{ "id": "r-1" }], "count": 3 });
        assert_eq!(array(&v, "rules").len(), 1);
        assert!(array(&v, "count").is_empty());
        assert!(array(&v, "absent").is_empty());
    }

    #[test]
    fn test_cell_unquotes_strings_and_compacts_other_values() {
        let v = json!({ "name": "nightly", "enabled": true, "n": 4, "gone": null });
        assert_eq!(cell(&v, "name"), "nightly");
        assert_eq!(cell(&v, "enabled"), "true");
        assert_eq!(cell(&v, "n"), "4");
        assert_eq!(cell(&v, "gone"), "");
        assert_eq!(cell(&v, "absent"), "");
    }

    #[test]
    fn test_rows_table_is_empty_for_no_rows() {
        assert_eq!(rows_table(&["ID"], Vec::new()), "");
    }
}
