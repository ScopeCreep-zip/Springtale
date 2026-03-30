pub mod create_post;
pub mod like;
pub mod reply;
pub mod repost;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the Bluesky connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        create_post::declaration(),
        reply::declaration(),
        like::declaration(),
        repost::declaration(),
    ]
}
