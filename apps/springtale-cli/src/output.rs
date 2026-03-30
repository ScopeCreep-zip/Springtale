use anyhow::Result;
use serde::Serialize;

/// Print data as formatted JSON to stdout.
pub fn print_json<T: Serialize>(data: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    println!("{json}");
    Ok(())
}
