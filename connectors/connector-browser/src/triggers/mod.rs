use springtale_connector::manifest::types::TriggerDecl;

pub fn trigger_declarations() -> Vec<TriggerDecl> {
    vec![page_loaded(), element_found()]
}

fn page_loaded() -> TriggerDecl {
    TriggerDecl {
        name: "page_loaded".to_owned(),
        description: "Fires when a navigated page finishes loading.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "title": { "type": "string" },
                "status": { "type": "integer" }
            },
            "required": ["url"]
        })),
    }
}

fn element_found() -> TriggerDecl {
    TriggerDecl {
        name: "element_found".to_owned(),
        description: "Fires when a CSS selector matches on the current page.".to_owned(),
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string" },
                "text": { "type": "string" },
                "found": { "type": "boolean" }
            },
            "required": ["selector", "found"]
        })),
    }
}
