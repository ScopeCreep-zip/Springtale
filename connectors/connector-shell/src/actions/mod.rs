pub mod exec;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the shell connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![exec::declaration()]
}
