use crate::error::StoreError;
use crate::schema::bot::{MemoryRow, SessionRow, UserPrefsRow};

use super::InMemoryBackend;

// ── Bot Sessions ──────────────────────────────────────────

impl InMemoryBackend {
    pub(super) async fn upsert_session_impl(&self, session: &SessionRow) -> Result<(), StoreError> {
        let mut sessions = self.sessions.write().await;
        let key = (session.user_id.clone(), session.channel_id.clone());
        sessions.insert(key, session.clone());
        Ok(())
    }

    pub(super) async fn get_session_impl(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<Option<SessionRow>, StoreError> {
        let sessions = self.sessions.read().await;
        let key = (user_id.to_owned(), channel_id.to_owned());
        Ok(sessions.get(&key).cloned())
    }

    pub(super) async fn delete_session_impl(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<(), StoreError> {
        let mut sessions = self.sessions.write().await;
        let key = (user_id.to_owned(), channel_id.to_owned());
        sessions.remove(&key);
        Ok(())
    }

    pub(super) async fn list_sessions_impl(&self) -> Result<Vec<SessionRow>, StoreError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values().cloned().collect())
    }

    // ── User Preferences ──────────────────────────────────────

    pub(super) async fn upsert_user_prefs_impl(
        &self,
        prefs: &UserPrefsRow,
    ) -> Result<(), StoreError> {
        let mut user_prefs = self.user_prefs.write().await;
        user_prefs.insert(prefs.user_id.clone(), prefs.clone());
        Ok(())
    }

    pub(super) async fn get_user_prefs_impl(
        &self,
        user_id: &str,
    ) -> Result<Option<UserPrefsRow>, StoreError> {
        let user_prefs = self.user_prefs.read().await;
        Ok(user_prefs.get(user_id).cloned())
    }

    // ── Bot Memory ────────────────────────────────────────────

    pub(super) async fn insert_memory_impl(&self, entry: &MemoryRow) -> Result<(), StoreError> {
        let mut memory = self.memory.write().await;
        memory.push(entry.clone());
        Ok(())
    }

    pub(super) async fn get_memory_impl(
        &self,
        user_id: &str,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRow>, StoreError> {
        let memory = self.memory.read().await;
        let mut matching: Vec<MemoryRow> = memory
            .iter()
            .filter(|m| m.user_id == user_id && m.channel_id == channel_id)
            .cloned()
            .collect();
        // Sort by created_at DESC (most recent first)
        matching.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        matching.truncate(limit);
        Ok(matching)
    }

    pub(super) async fn delete_memory_impl(
        &self,
        user_id: &str,
        channel_id: &str,
    ) -> Result<u64, StoreError> {
        let mut memory = self.memory.write().await;
        let before = memory.len();
        memory.retain(|m| !(m.user_id == user_id && m.channel_id == channel_id));
        Ok((before - memory.len()) as u64)
    }

    pub(super) async fn compact_memory_impl(
        &self,
        user_id: &str,
        channel_id: &str,
        max_entries: usize,
    ) -> Result<u64, StoreError> {
        let mut memory = self.memory.write().await;
        let mut matching: Vec<(usize, &MemoryRow)> = memory
            .iter()
            .enumerate()
            .filter(|(_, m)| m.user_id == user_id && m.channel_id == channel_id)
            .collect();
        // Sort by created_at DESC
        matching.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

        if matching.len() <= max_entries {
            return Ok(0);
        }

        // Indices to remove (oldest beyond max_entries)
        let to_remove: Vec<usize> = matching[max_entries..].iter().map(|(i, _)| *i).collect();
        let count = to_remove.len() as u64;

        // Remove in reverse index order to avoid shifting
        let mut to_remove_sorted = to_remove;
        to_remove_sorted.sort_unstable();
        for idx in to_remove_sorted.into_iter().rev() {
            memory.remove(idx);
        }

        Ok(count)
    }
}
