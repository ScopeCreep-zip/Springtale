pub mod discover_destinations;
pub mod send_group_message;
pub mod send_message;
pub mod set_disappearing_timer;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        send_message::declaration(),
        send_group_message::declaration(),
        set_disappearing_timer::declaration(),
        discover_destinations::declaration(),
    ]
}
