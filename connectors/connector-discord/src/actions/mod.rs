pub mod add_reaction;
pub mod delete_message;
pub mod edit_message;
pub mod send_embed;
pub mod send_message;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        send_message::declaration(),
        send_embed::declaration(),
        edit_message::declaration(),
        delete_message::declaration(),
        add_reaction::declaration(),
    ]
}
