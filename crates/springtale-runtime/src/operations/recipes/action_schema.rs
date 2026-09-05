//! Rendered `RunConnector` params checked against the connector action's
//! `input_schema` — the same check the MCP bridge applies to tool calls
//! (`springtale-mcp/src/server/handlers.rs`), applied at preflight and at
//! deploy so a recipe can never create a rule the connector rejects at
//! dispatch.

use springtale_connector::ActionDecl;
use springtale_core::rule::action::Action;
use springtale_core::rule::types::Rule;

/// Result of checking one rendered `RunConnector` step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionCheck {
    /// `"{rule}: {connector}.{action}"` — names the step in reports and errors.
    pub step: String,
    pub outcome: ActionOutcome,
}

/// What the check found for one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    /// The connector is not installed; `check_connectors` reports that.
    Skipped,
    /// The action exists and the params satisfy its schema (or it has none).
    Valid,
    /// Unknown action name or a schema violation — the reason to show.
    Invalid(String),
}

/// Check every `RunConnector` leaf of `rule` (walking `Chain` steps) against
/// the action declarations `lookup` returns for its connector.
pub fn check_rule_actions<'a>(
    rule: &Rule,
    lookup: impl Fn(&str) -> Option<&'a [ActionDecl]>,
) -> Vec<ActionCheck> {
    let mut out = Vec::new();
    for leaf in rule.actions.iter().flat_map(Action::iter_leaves) {
        let Action::RunConnector {
            connector,
            action,
            params,
        } = leaf
        else {
            continue;
        };
        let step = format!("{}: {connector}.{action}", rule.name);
        let outcome = match lookup(connector) {
            None => ActionOutcome::Skipped,
            Some(decls) => match decls.iter().find(|d| d.name == *action) {
                None => ActionOutcome::Invalid(format!(
                    "`{connector}` declares no action named `{action}`"
                )),
                Some(decl) => validate_params(decl, params),
            },
        };
        out.push(ActionCheck { step, outcome });
    }
    out
}

fn validate_params(
    decl: &ActionDecl,
    params: &serde_json::Map<String, serde_json::Value>,
) -> ActionOutcome {
    let Some(schema) = &decl.input_schema else {
        return ActionOutcome::Valid;
    };
    match jsonschema::validate(schema, &serde_json::Value::Object(params.clone())) {
        Ok(()) => ActionOutcome::Valid,
        Err(e) => ActionOutcome::Invalid(format!("params do not match the action schema: {e}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    fn decls() -> Vec<ActionDecl> {
        vec![ActionDecl {
            name: "send".into(),
            description: String::new(),
            input_schema: Some(json!({
                "type": "object",
                "required": ["text"],
                "properties": { "text": { "type": "string" }, "count": { "type": "integer" } }
            })),
            output_schema: None,
            read_only: false,
            destructive: None,
        }]
    }

    fn rule(action: &str, params: &str) -> Rule {
        let toml = format!(
            "name = \"t\"\n\n[trigger]\ntype = \"Cron\"\nexpression = \"0 * * * *\"\n\n\
             [[actions]]\ntype = \"Chain\"\n\n[[actions.steps]]\ntype = \"RunConnector\"\n\
             connector = \"connector-x\"\naction = \"{action}\"\n\n[actions.steps.params]\n{params}\n"
        );
        toml::from_str(&toml).expect("rule toml")
    }

    fn check(action: &str, params: &str) -> ActionOutcome {
        let decls = decls();
        let mut out = check_rule_actions(&rule(action, params), |name| {
            (name == "connector-x").then_some(decls.as_slice())
        });
        assert_eq!(out.len(), 1, "one leaf through the Chain");
        assert_eq!(out[0].step, "t: connector-x.send");
        out.remove(0).outcome
    }

    #[test]
    fn test_check_rule_actions_valid_params_verified() {
        assert_eq!(check("send", "text = \"hi\""), ActionOutcome::Valid);
    }

    #[test]
    fn test_check_rule_actions_unknown_action_invalid() {
        let outcome = check_rule_actions(&rule("nope", "text = \"hi\""), |_| Some(&[][..]));
        assert!(
            matches!(&outcome[0].outcome, ActionOutcome::Invalid(r) if r.contains("no action named `nope`"))
        );
    }

    #[test]
    fn test_check_rule_actions_missing_required_param_invalid() {
        assert!(
            matches!(check("send", "count = 1"), ActionOutcome::Invalid(r) if r.contains("text"))
        );
    }

    #[test]
    fn test_check_rule_actions_wrong_type_invalid() {
        assert!(matches!(
            check("send", "text = \"hi\"\ncount = \"many\""),
            ActionOutcome::Invalid(_)
        ));
    }

    #[test]
    fn test_check_rule_actions_connector_not_installed_skipped() {
        let out = check_rule_actions(&rule("send", "text = \"hi\""), |_| None);
        assert_eq!(out[0].outcome, ActionOutcome::Skipped);
    }
}
