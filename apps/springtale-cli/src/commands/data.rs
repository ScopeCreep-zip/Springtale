use anyhow::Result;
use springtale_store::backend::sqlite::SqliteBackend;

use crate::cli::DataAction;

/// Handle data subcommands.
pub async fn run(action: DataAction, store: &SqliteBackend) -> Result<()> {
    match action {
        DataAction::Export { output, encrypt } => {
            if encrypt {
                anyhow::bail!("encrypted export requires travel mode (springtale travel prepare)");
            }
            let data = springtale_runtime::operations::data::export_data(store)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let json = serde_json::to_string_pretty(&data)?;
            if let Some(path) = output {
                // Write with 0o600 permissions (architecture doc §8.2)
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&path)?;
                let mut writer = std::io::BufWriter::new(file);
                writer.write_all(json.as_bytes())?;
                eprintln!("Exported to: {}", path.display());
            } else {
                println!("{json}");
            }
        }
        DataAction::Purge => {
            springtale_runtime::operations::data::purge_data(store)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            eprintln!("All user data purged. Vault intact.");
        }
    }
    Ok(())
}
