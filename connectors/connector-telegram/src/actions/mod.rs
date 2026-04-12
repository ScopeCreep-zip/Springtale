pub mod answer_callback_query;
pub mod delete_message;
pub mod edit_message;
pub mod send_inline_keyboard;
pub mod send_message;
pub mod send_photo;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        send_message::declaration(),
        send_photo::declaration(),
        edit_message::declaration(),
        delete_message::declaration(),
        send_inline_keyboard::declaration(),
        answer_callback_query::declaration(),
    ]
}
