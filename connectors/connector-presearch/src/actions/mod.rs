pub mod scrape;
pub mod search;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the Presearch connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![search::declaration(), scrape::declaration()]
}
