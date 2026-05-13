pub mod discover_destinations;
pub mod join_channel;
pub mod part_channel;
pub mod send_action;
pub mod send_message;
pub mod set_topic;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        send_message::declaration(),
        join_channel::declaration(),
        part_channel::declaration(),
        set_topic::declaration(),
        send_action::declaration(),
        discover_destinations::declaration(),
    ]
}
