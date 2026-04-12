pub mod publish_note;
pub mod react;
pub mod reply;
pub mod send_dm;
pub mod send_message;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        publish_note::declaration(),
        send_dm::declaration(),
        send_message::declaration(),
        react::declaration(),
        reply::declaration(),
    ]
}
