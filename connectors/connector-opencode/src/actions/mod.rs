pub mod continue_session;
pub mod run_task;

#[cfg(test)]
pub mod test_support;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the OpenCode connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![run_task::declaration(), continue_session::declaration()]
}
