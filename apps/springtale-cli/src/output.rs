use anyhow::Result;
use serde::Serialize;

/// Print data as formatted JSON to stdout.
pub fn print_json<T: Serialize>(data: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
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
