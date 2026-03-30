pub mod get_channel;
pub mod get_stream;
pub mod send_chat;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the Kick connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        send_chat::declaration(),
        get_channel::declaration(),
        get_stream::declaration(),
    ]
}
