use anyhow::{Context, Result};

use crate::output;

/// Start springtaled as a child process (development mode).
///
/// Looks for the `springtaled` binary in PATH or the same directory
/// as the CLI binary. Forwards SIGTERM for graceful shutdown.
pub async fn run(json_out: bool) -> Result<()> {
    // Find the springtaled binary — check same directory as CLI first
    let springtaled_path = find_springtaled()?;

    let starting = starting_body(&springtaled_path);
    output::emit(json_out, &starting, |_| {
        "Starting springtaled...".to_owned()
    })?;

    tracing::info!(path = %springtaled_path.display(), "launching springtaled");

    let mut child = tokio::process::Command::new(&springtaled_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to start {}", springtaled_path.display()))?;

    // Wait for the child process
    let status = child
        .wait()
        .await
        .context("failed to wait for springtaled")?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("springtaled exited with code {code}");
    }
    let exited = exited_body(0);
    output::emit(json_out, &exited, |_| {
        "springtaled exited cleanly".to_owned()
    })
}

/// Find the springtaled binary.
fn find_springtaled() -> Result<std::path::PathBuf> {
    // Check same directory as the running CLI binary (common in cargo builds)
    if let Ok(self_path) = std::env::current_exe()
        && let Some(dir) = self_path.parent()
    {
        let candidate = dir.join("springtaled");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    // Fall back to assuming it's in PATH
    Ok(std::path::PathBuf::from("springtaled"))
}

/// The `--json` body emitted before springtaled is spawned.
fn starting_body(binary: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "status": "starting",
        "binary": binary.display().to_string(),
    })
}

/// The `--json` body emitted after a clean exit.
fn exited_body(code: i32) -> serde_json::Value {
    serde_json::json!({ "status": "exited", "code": code })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{json_value, key_set};

    #[test]
    fn test_server_start_json_shape_names_status_and_binary() {
        let out = json_value(&starting_body(std::path::Path::new("/usr/bin/springtaled")));
        assert_eq!(key_set(&out), ["binary", "status"]);
        assert_eq!(out["status"], "starting");
        assert!(out["binary"].is_string());
        assert_eq!(out["binary"], "/usr/bin/springtaled");
    }

    #[test]
    fn test_server_exit_json_shape_names_status_and_code() {
        let out = json_value(&exited_body(0));
        assert_eq!(key_set(&out), ["code", "status"]);
        assert_eq!(out["status"], "exited");
        assert!(out["code"].is_number());
        assert_eq!(out["code"], 0);
    }
}
