use chrono::{DateTime, Utc};

use crate::error::StoreError;
use crate::schema::events::{EventEntry, EventFilter};

use super::InMemoryBackend;

impl InMemoryBackend {
    pub(super) async fn log_event_impl(&self, event: &EventEntry) -> Result<(), StoreError> {
        let mut events = self.events.write().await;
        events.push(event.clone());
        Ok(())
    }

    pub(super) async fn list_events_impl(
        &self,
        filter: &EventFilter,
    ) -> Result<Vec<EventEntry>, StoreError> {
        let events = self.events.read().await;
        let filtered: Vec<EventEntry> = events
            .iter()
            .filter(|e| {
                if filter
                    .trigger_type
                    .as_ref()
                    .is_some_and(|tt| e.trigger_type != *tt)
                {
                    return false;
                }
                if filter.after.as_ref().is_some_and(|a| e.timestamp < *a) {
                    return false;
                }
                if filter.before.as_ref().is_some_and(|b| e.timestamp > *b) {
                    return false;
                }
                true
            })
            .take(filter.limit.unwrap_or(100) as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }

    pub(super) async fn delete_events_before_impl(
        &self,
        before: &DateTime<Utc>,
    ) -> Result<u64, StoreError> {
        let mut events = self.events.write().await;
        let before_len = events.len();
        events.retain(|e| e.timestamp >= *before);
        Ok((before_len - events.len()) as u64)
    }
}
