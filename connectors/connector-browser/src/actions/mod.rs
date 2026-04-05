pub mod click;
pub mod extract_text;
pub mod fill_form;
pub mod navigate;
pub mod screenshot;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        navigate::declaration(),
        fill_form::declaration(),
        click::declaration(),
        screenshot::declaration(),
        extract_text::declaration(),
    ]
}
