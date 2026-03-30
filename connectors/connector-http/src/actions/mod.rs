pub mod get;
pub mod post;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the HTTP connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![get::declaration(), post::declaration()]
}
