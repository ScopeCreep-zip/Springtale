//! What the command line is, in machine-readable form (plan 2.3).
//!
//! `scripts/check-surface.sh` has to answer one question: does every
//! route the daemon serves have a command-line verb? Reading that off
//! the *sources* — grepping for path literals — answers a different and
//! weaker question. A path in a comment counts. A verb that builds its
//! path from a constant or a `match` does not. The list is a guess about
//! code, not a statement by the program.
//!
//! So the program states it. `springtale dump-commands` prints its own
//! command tree, walked out of clap at runtime, with the daemon routes
//! each verb calls attached. The tree half cannot drift: it *is* the
//! parser. The route half is declared here beside the verb, and
//! [`tests`] fails the build if a verb has no declaration or a
//! declaration has no verb — so a new subcommand cannot be added
//! without saying what it talks to, and a deleted one cannot leave a
//! ghost behind.
//!
//! A verb with an empty route list is a deliberate statement too: it
//! runs offline, against the vault and the local store, with no daemon
//! in the picture (plan 2.2's offline set).

use clap::CommandFactory;

use crate::cli::Cli;

/// One command-line verb and the daemon routes it calls.
pub struct VerbRoutes {
    /// The full verb path, exactly as it is typed: `formation rally`.
    pub verb: &'static str,
    /// The routes it calls, as the client sees them — `{id}`-style
    /// holes and no query string. Empty means the verb is offline.
    pub routes: &'static [&'static str],
}

/// Every verb, and what it talks to.
pub const VERB_ROUTES: &[VerbRoutes] = &[
    VerbRoutes {
        verb: "agent set-autonomy",
        routes: &["/agents/{name}/autonomy"],
    },
    VerbRoutes {
        verb: "agent states",
        routes: &["/agents/states"],
    },
    VerbRoutes {
        verb: "agent step-autonomy",
        routes: &["/agents/{name}/autonomy/step"],
    },
    VerbRoutes {
        verb: "approval approve",
        routes: &["/approvals/{id}"],
    },
    VerbRoutes {
        verb: "approval deny",
        routes: &["/approvals/{id}"],
    },
    VerbRoutes {
        verb: "approval list",
        routes: &["/approvals"],
    },
    VerbRoutes {
        verb: "auth revoke",
        routes: &["/auth/tokens/{id}"],
    },
    VerbRoutes {
        verb: "auth tokens",
        routes: &["/auth/tokens"],
    },
    VerbRoutes {
        verb: "author add",
        routes: &[],
    },
    VerbRoutes {
        verb: "author list",
        routes: &[],
    },
    VerbRoutes {
        verb: "author remove",
        routes: &[],
    },
    VerbRoutes {
        verb: "bot formations",
        routes: &["/bot/formations"],
    },
    VerbRoutes {
        verb: "bot memory",
        routes: &["/bot/memory"],
    },
    VerbRoutes {
        verb: "bot pair-init",
        routes: &["/bot/pair-init"],
    },
    VerbRoutes {
        verb: "bot panic-unpair",
        routes: &[],
    },
    VerbRoutes {
        verb: "bot settings get",
        routes: &["/bot/settings"],
    },
    VerbRoutes {
        verb: "bot settings set",
        routes: &["/bot/settings"],
    },
    VerbRoutes {
        verb: "bot status",
        routes: &["/bot/status"],
    },
    VerbRoutes {
        verb: "canvas",
        routes: &[
            "/canvas",
            "/canvas/connections",
            "/stream",
            "/stream/ticket",
        ],
    },
    VerbRoutes {
        verb: "chat",
        routes: &["/chat"],
    },
    VerbRoutes {
        verb: "config ai get",
        routes: &["/config/{key}"],
    },
    VerbRoutes {
        verb: "config ai put",
        routes: &["/config/ai"],
    },
    VerbRoutes {
        verb: "config ai set",
        routes: &["/config/ai/configure"],
    },
    VerbRoutes {
        verb: "config connector",
        routes: &["/config/connector/{name}"],
    },
    VerbRoutes {
        verb: "config heartbeat",
        routes: &["/config/heartbeat"],
    },
    VerbRoutes {
        verb: "config list",
        routes: &["/config"],
    },
    VerbRoutes {
        verb: "connector available",
        routes: &["/connectors/available"],
    },
    VerbRoutes {
        verb: "connector cascade",
        routes: &["/connectors/{name}/cascade"],
    },
    VerbRoutes {
        verb: "connector config",
        routes: &["/connectors/{name}/config"],
    },
    VerbRoutes {
        verb: "connector disable",
        routes: &["/connectors/{name}/disable"],
    },
    VerbRoutes {
        verb: "connector enable",
        routes: &["/connectors/{name}/enable"],
    },
    VerbRoutes {
        verb: "connector install",
        routes: &["/connectors/install"],
    },
    VerbRoutes {
        verb: "connector install-wasm",
        routes: &["/connectors/install-wasm"],
    },
    VerbRoutes {
        verb: "connector list",
        routes: &["/connectors"],
    },
    VerbRoutes {
        verb: "connector outputs",
        routes: &["/connectors/{name}/outputs"],
    },
    VerbRoutes {
        verb: "connector reload",
        routes: &["/connectors/{name}/reload"],
    },
    VerbRoutes {
        verb: "connector remove",
        routes: &["/connectors/{name}"],
    },
    VerbRoutes {
        verb: "connector schemas",
        routes: &["/connectors/schemas"],
    },
    VerbRoutes {
        verb: "connector setup",
        routes: &["/connectors/setup"],
    },
    VerbRoutes {
        verb: "connector sign",
        routes: &[],
    },
    VerbRoutes {
        verb: "connector test",
        routes: &["/connectors/{name}/test"],
    },
    VerbRoutes {
        verb: "connector upsert-config",
        routes: &["/connectors/{name}/upsert-config"],
    },
    VerbRoutes {
        verb: "cooperation glyphs",
        routes: &[],
    },
    VerbRoutes {
        verb: "cooperation recent",
        routes: &["/cooperation/utterances/recent"],
    },
    VerbRoutes {
        verb: "cooperation utterances",
        routes: &["/cooperation/utterances"],
    },
    VerbRoutes {
        verb: "crypto rotate-vault-key",
        routes: &[],
    },
    VerbRoutes {
        verb: "data export",
        routes: &["/data/export"],
    },
    VerbRoutes {
        verb: "data import",
        routes: &["/data/import"],
    },
    VerbRoutes {
        verb: "data purge",
        routes: &["/data/purge"],
    },
    VerbRoutes {
        verb: "doctor",
        routes: &[],
    },
    VerbRoutes {
        verb: "drift recipe",
        routes: &["/drift/recipe/{id}"],
    },
    VerbRoutes {
        verb: "drift rule",
        routes: &["/drift/rule/{id}"],
    },
    VerbRoutes {
        verb: "events",
        routes: &["/events"],
    },
    VerbRoutes {
        verb: "execution list",
        routes: &["/executions"],
    },
    VerbRoutes {
        verb: "execution steps",
        routes: &["/executions/{id}/steps"],
    },
    VerbRoutes {
        verb: "execution vacuum",
        routes: &["/executions/vacuum"],
    },
    VerbRoutes {
        verb: "fix",
        routes: &[],
    },
    VerbRoutes {
        verb: "formation add-member",
        routes: &["/formations/{id}/members"],
    },
    VerbRoutes {
        verb: "formation autonomy",
        routes: &["/formations/{id}/cycle-autonomy"],
    },
    VerbRoutes {
        verb: "formation commands",
        routes: &["/formations/{id}/commands"],
    },
    VerbRoutes {
        verb: "formation deploy",
        routes: &["/formations/{id}/deploy"],
    },
    VerbRoutes {
        verb: "formation deploy-team",
        routes: &["/formations/deploy-team"],
    },
    VerbRoutes {
        verb: "formation dissolve",
        routes: &["/formations/{id}/dissolve"],
    },
    VerbRoutes {
        verb: "formation eligible",
        routes: &["/formations/{id}/members/eligible"],
    },
    VerbRoutes {
        verb: "formation get",
        routes: &["/formations/{id}"],
    },
    VerbRoutes {
        verb: "formation guard",
        routes: &["/formations/{id}/toggle-guard"],
    },
    VerbRoutes {
        verb: "formation intent",
        routes: &["/formations/{id}/cycle-intent", "/formations/{id}/intent"],
    },
    VerbRoutes {
        verb: "formation intents",
        routes: &["/formations/intents"],
    },
    VerbRoutes {
        verb: "formation list",
        routes: &["/formations"],
    },
    VerbRoutes {
        verb: "formation pause",
        routes: &["/formations/{id}/pause"],
    },
    VerbRoutes {
        verb: "formation propose-intent",
        routes: &["/formations/{id}/propose-intent"],
    },
    VerbRoutes {
        verb: "formation rally",
        routes: &["/formations/{id}/rally"],
    },
    VerbRoutes {
        verb: "formation resume",
        routes: &["/formations/{id}/resume"],
    },
    VerbRoutes {
        verb: "formation rm-member",
        routes: &["/formations/{id}/members"],
    },
    VerbRoutes {
        verb: "formation run",
        routes: &["/formations/{id}/run-command"],
    },
    VerbRoutes {
        verb: "formation vote",
        routes: &["/formations/{id}/votes/{vote_id}"],
    },
    VerbRoutes {
        verb: "healthcheck",
        routes: &["/health", "/ready"],
    },
    VerbRoutes {
        verb: "init",
        routes: &[],
    },
    VerbRoutes {
        verb: "login",
        routes: &["/auth/login"],
    },
    VerbRoutes {
        verb: "logout",
        routes: &["/auth/logout"],
    },
    VerbRoutes {
        verb: "mcp serve",
        routes: &["/mcp"],
    },
    VerbRoutes {
        verb: "memory audit",
        routes: &["/memory/audit"],
    },
    VerbRoutes {
        verb: "memory compact",
        routes: &["/memory/compact"],
    },
    VerbRoutes {
        verb: "onboarding apply",
        routes: &["/onboarding/{platform}"],
    },
    VerbRoutes {
        verb: "onboarding platforms",
        routes: &["/onboarding/platforms"],
    },
    VerbRoutes {
        verb: "panic",
        routes: &[],
    },
    VerbRoutes {
        verb: "recipe apply",
        routes: &["/recipes/{id}/apply"],
    },
    VerbRoutes {
        verb: "recipe categories",
        routes: &["/recipes/categories"],
    },
    VerbRoutes {
        verb: "recipe delete",
        routes: &["/recipes/user/{id}"],
    },
    VerbRoutes {
        verb: "recipe export",
        routes: &["/recipes/{id}/export"],
    },
    VerbRoutes {
        verb: "recipe favorite",
        routes: &["/recipes/{id}/favorite"],
    },
    VerbRoutes {
        verb: "recipe fork",
        routes: &["/recipes/{id}/fork"],
    },
    VerbRoutes {
        verb: "recipe get",
        routes: &["/recipes/{id}"],
    },
    VerbRoutes {
        verb: "recipe import",
        routes: &["/recipes/import"],
    },
    VerbRoutes {
        verb: "recipe list",
        routes: &["/recipes"],
    },
    VerbRoutes {
        verb: "recipe pieces",
        routes: &["/recipes/{id}/pieces"],
    },
    VerbRoutes {
        verb: "recipe preflight",
        routes: &["/recipes/{id}/preflight"],
    },
    VerbRoutes {
        verb: "recipe preview",
        routes: &["/recipes/{id}/preview"],
    },
    VerbRoutes {
        verb: "recipe recent",
        routes: &["/recipes/{id}/recent"],
    },
    VerbRoutes {
        verb: "recipe render",
        routes: &["/recipes/{id}/render"],
    },
    VerbRoutes {
        verb: "recipe save",
        routes: &["/recipes/user"],
    },
    VerbRoutes {
        verb: "recipe test-step",
        routes: &["/recipes/{id}/test-step"],
    },
    VerbRoutes {
        verb: "rule add",
        routes: &["/rules"],
    },
    VerbRoutes {
        verb: "rule add-for-connector",
        routes: &["/rules/connector"],
    },
    VerbRoutes {
        verb: "rule delete",
        routes: &["/rules/{id}"],
    },
    VerbRoutes {
        verb: "rule for-connector",
        routes: &["/rules/connector/{name}"],
    },
    VerbRoutes {
        verb: "rule list",
        routes: &["/rules"],
    },
    VerbRoutes {
        verb: "rule move",
        routes: &["/rules/{id}/reassign"],
    },
    VerbRoutes {
        verb: "rule parse",
        routes: &["/rules/parse"],
    },
    VerbRoutes {
        verb: "rule run",
        routes: &["/rules/{id}/run"],
    },
    VerbRoutes {
        verb: "rule schema",
        routes: &["/rules/schema"],
    },
    VerbRoutes {
        verb: "rule toggle",
        routes: &["/rules", "/rules/{id}/toggle"],
    },
    VerbRoutes {
        verb: "rule update",
        routes: &["/rules/{id}"],
    },
    VerbRoutes {
        verb: "run",
        routes: &[],
    },
    VerbRoutes {
        verb: "safety disguise",
        routes: &["/safety/disguise/active"],
    },
    VerbRoutes {
        verb: "safety disguise-profile",
        routes: &["/safety/disguise/profile"],
    },
    VerbRoutes {
        verb: "safety get",
        routes: &["/safety"],
    },
    VerbRoutes {
        verb: "safety panic-taps",
        routes: &["/safety/panic_tap_count"],
    },
    VerbRoutes {
        verb: "send",
        routes: &["/send"],
    },
    VerbRoutes {
        verb: "server start",
        routes: &[],
    },
    VerbRoutes {
        verb: "session list",
        routes: &["/sessions"],
    },
    VerbRoutes {
        verb: "trace",
        routes: &["/stream", "/stream/ticket"],
    },
    VerbRoutes {
        verb: "travel prepare",
        routes: &[],
    },
    VerbRoutes {
        verb: "travel restore",
        routes: &[],
    },
    VerbRoutes {
        verb: "vault duress-setup",
        routes: &[],
    },
    VerbRoutes {
        verb: "vault unlock",
        routes: &["/vault/unlock"],
    },
    VerbRoutes {
        verb: "workspace add",
        routes: &["/workspaces"],
    },
    VerbRoutes {
        verb: "workspace list",
        routes: &["/workspaces"],
    },
    VerbRoutes {
        verb: "workspace onboard",
        routes: &["/workspaces/onboard"],
    },
    VerbRoutes {
        verb: "workspace onboard-url",
        routes: &["/workspaces/onboard-url"],
    },
    VerbRoutes {
        verb: "workspace remove",
        routes: &["/workspaces"],
    },
    VerbRoutes {
        verb: "workspace scan",
        routes: &["/workspaces/scan"],
    },
];

/// The full verb path of every leaf subcommand, sorted.
///
/// Hidden subcommands and clap's generated `help` are not part of the
/// product surface and are skipped.
pub fn verbs() -> Vec<String> {
    let mut out = Vec::new();
    collect(&Cli::command(), "", &mut out);
    out.sort();
    out
}

/// Walk one node of the clap tree, pushing leaves onto `out`.
fn collect(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
    let children: Vec<&clap::Command> = cmd
        .get_subcommands()
        .filter(|c| !c.is_hide_set() && c.get_name() != "help")
        .collect();

    if children.is_empty() {
        if !prefix.is_empty() {
            out.push(prefix.to_owned());
        }
        return;
    }

    for child in children {
        let verb = if prefix.is_empty() {
            child.get_name().to_owned()
        } else {
            format!("{prefix} {}", child.get_name())
        };
        collect(child, &verb, out);
    }
}

/// The routes declared for one verb, or `None` when it has none
/// declared — which the test below does not allow to happen.
fn routes_for(verb: &str) -> Option<&'static [&'static str]> {
    VERB_ROUTES
        .iter()
        .find(|entry| entry.verb == verb)
        .map(|entry| entry.routes)
}

/// The dump `springtale dump-commands` prints.
pub fn dump() -> serde_json::Value {
    let commands: Vec<serde_json::Value> = verbs()
        .into_iter()
        .map(|verb| {
            let routes = routes_for(&verb);
            serde_json::json!({ "verb": verb, "routes": routes })
        })
        .collect();
    serde_json::json!({ "commands": commands })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verb_routes_covers_every_verb_exactly() {
        let verbs = verbs();
        let missing: Vec<&String> = verbs.iter().filter(|v| routes_for(v).is_none()).collect();
        assert!(
            missing.is_empty(),
            "verbs with no declared routes (add them to VERB_ROUTES; an offline verb declares an empty list): {missing:?}"
        );

        let stale: Vec<&str> = VERB_ROUTES
            .iter()
            .map(|e| e.verb)
            .filter(|v| !verbs.iter().any(|known| known == v))
            .collect();
        assert!(
            stale.is_empty(),
            "VERB_ROUTES entries for verbs that no longer exist: {stale:?}"
        );
    }

    #[test]
    fn test_declared_routes_are_absolute_paths() {
        for entry in VERB_ROUTES {
            for route in entry.routes {
                assert!(
                    route.starts_with('/') && !route.contains('?'),
                    "{}: `{route}` is not a query-free absolute path",
                    entry.verb
                );
            }
        }
    }
}
