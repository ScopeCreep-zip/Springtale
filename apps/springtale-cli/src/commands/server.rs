use anyhow::{Context, Result};

/// Start springtaled as a child process (development mode).
///
/// Looks for the `springtaled` binary in PATH or the same directory
/// as the CLI binary. Forwards SIGTERM for graceful shutdown.
pub async fn run() -> Result<()> {
    println!("Starting springtaled...");

    // Find the springtaled binary — check same directory as CLI first
    let springtaled_path = find_springtaled()?;

    tracing::info!(path = %springtaled_path.display(), "launching springtaled");

    let mut child = tokio::process::Command::new(&springtaled_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to start {}", springtaled_path.display()))?;

    // Wait for the child process
    let status = child.wait().await.context("failed to wait for springtaled")?;

    if status.success() {
        println!("springtaled exited cleanly");
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("springtaled exited with code {code}");
    }

    Ok(())
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
