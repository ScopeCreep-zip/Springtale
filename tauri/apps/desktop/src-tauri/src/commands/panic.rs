use tauri::State;

use crate::state::AppState;

/// Emergency data destruction — panic wipe.
///
/// Per ARCHITECTURE.md §2.6: must complete within 3 seconds.
/// Delegates to shared runtime operation, then exits process.
#[tauri::command]
pub async fn panic_wipe(state: State<'_, AppState>) -> Result<(), String> {
    let store = state.runtime.store.clone();

    tokio::task::spawn_blocking(move || {
        // Use the shared panic_wipe operation (sync internally)
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            springtale_runtime::operations::safety::panic_wipe(store.as_ref())
                .await
                .map_err(|e| format!("wipe failed: {e}"))
        })
    })
    .await
    .map_err(|e| format!("wipe task failed: {e}"))??;

    std::process::exit(0);
}
