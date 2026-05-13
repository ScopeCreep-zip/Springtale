pub mod add_reaction;
pub mod discover_destinations;
pub mod edit_message;
pub mod send_blocks;
pub mod send_message;
pub mod send_thread_reply;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        send_message::declaration(),
        send_blocks::declaration(),
        send_thread_reply::declaration(),
        edit_message::declaration(),
        add_reaction::declaration(),
        discover_destinations::declaration(),
    ]
}
