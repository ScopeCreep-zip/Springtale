pub mod click;
pub mod evaluate;
pub mod extract_text;
pub mod fill_form;
pub mod get_html;
pub mod navigate;
pub mod query_all;
pub mod screenshot;
pub mod wait_for_selector;

use springtale_connector::manifest::types::ActionDecl;

pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        navigate::declaration(),
        fill_form::declaration(),
        click::declaration(),
        screenshot::declaration(),
        extract_text::declaration(),
        // Phase B — page-function primitives. Each is independently
        // invocable from rule TOML; recipes compose them into chains
        // (e.g. navigate → wait_for_selector → get_html → Extract).
        evaluate::declaration(),
        get_html::declaration(),
        query_all::declaration(),
        wait_for_selector::declaration(),
    ]
}
