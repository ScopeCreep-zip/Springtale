pub mod create_issue;
pub mod get_diff;
pub mod post_comment;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the GitHub connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        create_issue::declaration(),
        post_comment::declaration(),
        get_diff::declaration(),
    ]
}
