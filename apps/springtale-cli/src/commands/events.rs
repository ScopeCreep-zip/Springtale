use anyhow::Result;
use tabled::{Table, Tabled};

use springtale_store::backend::sqlite::SqliteBackend;
use springtale_store::backend::trait_::StorageBackend;
use springtale_store::schema::events::EventFilter;

use crate::output;

/// Row type for the events table.
#[derive(Tabled)]
struct EventTableRow {
    #[tabled(rename = "TIMESTAMP")]
    timestamp: String,
    #[tabled(rename = "CONNECTOR")]
    connector: String,
    #[tabled(rename = "TRIGGER")]
    trigger: String,
    #[tabled(rename = "ACTION")]
    action: String,
}

/// Display event log.
pub async fn run(
    store: &SqliteBackend,
    limit: u32,
    connector: Option<String>,
    json: bool,
) -> Result<()> {
    let filter = EventFilter {
        connector_name: connector,
        limit: Some(limit),
        ..Default::default()
    };

    let events = store.list_events(&filter).await?;

    if json {
        output::print_json(&events)?;
    } else if events.is_empty() {
        println!("No events recorded.");
    } else {
        let rows: Vec<EventTableRow> = events
            .iter()
            .map(|e| EventTableRow {
                timestamp: e.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                connector: e.connector_name.clone(),
                trigger: e.trigger_type.clone(),
                action: e.action_taken.clone(),
            })
            .collect();
        let table = Table::new(rows).to_string();
        println!("{table}");
    }

    Ok(())
}
