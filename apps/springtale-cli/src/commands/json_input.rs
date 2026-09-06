//! Reading a JSON request body from a file or from stdin.
//!
//! Several daemon routes take a free-form JSON object the CLI has no
//! business re-typing (connector config, recipe inputs, a send request).
//! Every one of them reads it the same way, so the reading lives here.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// Read a JSON object from `path`, or from stdin when `path` is `-`.
pub fn load(path: &Path) -> Result<Value> {
    let text = if path == Path::new("-") {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading JSON from stdin")?;
        buf
    } else {
        std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?
    };
    serde_json::from_str(&text).with_context(|| format!("{} must be JSON", path.display()))
}

/// Read an optional JSON object, defaulting to `{}`.
pub fn load_or_empty(path: Option<PathBuf>) -> Result<Value> {
    match path {
        Some(p) => load(&p),
        None => Ok(json!({})),
    }
}
