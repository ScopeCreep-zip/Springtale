//! Plan §6.9 — one way to start. Recipes are the starter system, so the
//! parallel template system must not come back: no `operations::templates`
//! module, no `springtale new` command, no `/templates` routes, no
//! template entry in the frontend data-provider interface.
//!
//! This is a source-level test rather than a route test because the point
//! is the absence of code, not the behaviour of code that exists.

use std::path::{Path, PathBuf};

/// Workspace root, two levels up from `apps/springtaled`.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn exists(rel: &str) -> bool {
    workspace_root().join(rel).exists()
}

#[test]
fn test_template_modules_no_longer_exist() {
    for rel in [
        "crates/springtale-runtime/src/operations/templates.rs",
        "apps/springtaled/src/api/templates.rs",
        "apps/springtale-cli/src/commands/new.rs",
        "docs/guide/templates.md",
    ] {
        assert!(!exists(rel), "{rel} should have been deleted");
    }
}

#[test]
fn test_template_modules_are_not_declared() {
    assert!(
        !read("crates/springtale-runtime/src/operations/mod.rs").contains("pub mod templates;"),
        "operations still declares the templates module"
    );
    assert!(
        !read("apps/springtaled/src/api/mod.rs").contains("pub mod templates;"),
        "api still declares the templates module"
    );
    assert!(
        !read("apps/springtale-cli/src/commands/mod.rs").contains("pub mod new;"),
        "cli still declares the new command module"
    );
}

#[test]
fn test_no_template_routes_remain() {
    let router = read("apps/springtaled/src/api/mod.rs");
    assert!(
        !router.contains("\"/templates\""),
        "GET /templates route still registered"
    );
    assert!(
        !router.contains("\"/templates/{name}\""),
        "POST /templates/{{name}} route still registered"
    );
}

#[test]
fn test_no_new_command_remains() {
    let cli = read("apps/springtale-cli/src/cli.rs");
    assert!(!cli.contains("    New {"), "Command::New still declared");
    assert!(
        !cli.contains("template: Option<String>"),
        "Command::Init still takes a template argument"
    );
    assert!(
        !read("apps/springtale-cli/src/main.rs").contains("Command::New"),
        "main still dispatches Command::New"
    );
}

#[test]
fn test_frontend_interface_has_no_template_item() {
    let types = read("tauri/packages/ui/src/dashboard/types.ts");
    assert!(
        !types.contains("listTemplates") && !types.contains("writeTemplate"),
        "DataProvider still declares template methods"
    );
    assert!(
        !read("tauri/packages/ui/src/web/provider.ts").contains("listTemplates"),
        "web provider still implements listTemplates"
    );
    assert!(
        !read("tauri/packages/types/src/operations.ts").contains("interface Template"),
        "shared types still declare Template"
    );
}

#[test]
fn test_scaffolds_that_became_recipes_are_in_the_catalog() {
    // §6.9: cron-runner, github-monitor, llm-assistant and llm-swarm map
    // onto the recipe catalogue rather than onto a scaffold generator.
    let ids: Vec<String> = springtale_runtime::operations::recipes::builtin::all()
        .into_iter()
        .map(|r| r.id)
        .collect();
    for id in [
        "cron-runner",
        "github-monitor",
        "llm-assistant",
        "llm-swarm",
    ] {
        assert!(ids.iter().any(|i| i == id), "recipe {id} missing");
    }
}
