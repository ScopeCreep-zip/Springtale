/// Return the rule schema — trigger, condition, and action type definitions.
///
/// Single source of truth for all frontends (desktop, dashboard, CLI).
/// Used by the visual rule builder to generate input forms.
pub fn get_rule_schema() -> serde_json::Value {
    serde_json::json!({
        "triggers": {
            "Cron": { "fields": { "expression": { "type": "string", "description": "Cron expression (6 fields)" } } },
            "FileWatch": { "fields": { "path": { "type": "string" }, "event": { "type": "string", "enum": ["create", "modify", "delete"] } } },
            "Webhook": { "fields": { "path": { "type": "string" } } },
            "ConnectorEvent": { "fields": { "connector": { "type": "string" }, "event": { "type": "string" } } },
            "SystemEvent": { "fields": { "event": { "type": "string" } } },
            "Heartbeat": { "fields": {} },
        },
        "conditions": {
            "FieldEquals": { "fields": { "field": { "type": "string" }, "value": { "type": "any" } } },
            "Contains": { "fields": { "field": { "type": "string" }, "value": { "type": "string" } } },
            "Regex": { "fields": { "field": { "type": "string" }, "pattern": { "type": "string" } } },
            "TimeInRange": { "fields": { "start": { "type": "string", "description": "HH:MM" }, "end": { "type": "string" } } },
            "DayOfWeek": { "fields": { "days": { "type": "array", "items": { "type": "integer", "min": 0, "max": 6 } } } },
        },
        "actions": {
            "RunConnector": { "fields": { "connector": { "type": "string" }, "action": { "type": "string" }, "params": { "type": "object" } } },
            "SendMessage": { "fields": { "text": { "type": "string" } } },
            "WriteFile": { "fields": { "destination": { "type": "string" }, "content": { "type": "string" } } },
            "Notify": { "fields": { "title": { "type": "string" }, "body": { "type": "string" } } },
            "Delay": { "fields": { "seconds": { "type": "integer" } } },
            "AiComplete": { "fields": { "prompt": { "type": "string" }, "adapter": { "type": "string", "optional": true } } },
        },
    })
}
