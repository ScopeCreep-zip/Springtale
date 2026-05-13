//! External-workspace directory operations (D1).
//!
//! Read-side helpers + the universal harvester that runs on every
//! dispatched connector event. The destination registry itself
//! lives in `springtale-cooperation::mental_model::external_workspaces`
//! (the in-memory type) and the `mental_model_workspaces` SQL
//! table in `springtale-store`.

pub mod harvester;
pub mod query;
pub mod stream;

pub use harvester::harvest_event;
pub use query::{
    delete_workspace, list_workspaces, preview_onboard_url, scan_workspaces,
    upsert_workspace_manual, WorkspaceInfo,
};
pub use stream::{start_onboard_stream, OnDiscoveryCallback};
