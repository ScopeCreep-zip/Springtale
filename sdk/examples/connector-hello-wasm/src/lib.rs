//! Hello World WASM connector for Springtale — a WASI Preview 2
//! component built against the SDK's WIT world.
//!
//! Demonstrates the minimum viable community connector:
//! - two actions ("greet", "echo"), both read-only
//! - the `springtale:connector/guest` export the host calls
//! - no host imports, because `manifest.toml` declares no capabilities
//!
//! World: `sdk/connector-sdk/wit/connector.wit`.
//!
//! Build:  `cargo build --release --target wasm32-wasip2`
//! Output: `target/wasm32-wasip2/release/connector_hello_wasm.wasm`
//!         (already a component — the wasip2 target emits one directly,
//!         no `wasm-tools component new` step)
//! Install: copy the `.wasm` plus `manifest.toml` into Springtale and
//!         call `install_wasm_connector()`.

wit_bindgen::generate!({
    path: "../../connector-sdk/wit",
    world: "connector",
});

use exports::springtale::connector::guest::{ActionDecl, ActionResult, Guest};

/// Convenience constructors mirroring the SDK's `ActionResult` helpers.
fn ok(output: serde_json::Value, message: &str) -> ActionResult {
    ActionResult {
        success: true,
        output: output.to_string(),
        message: message.to_owned(),
    }
}

fn err(message: String) -> ActionResult {
    ActionResult {
        success: false,
        output: "null".to_owned(),
        message,
    }
}

/// The "greet" action — takes a name, returns a greeting.
fn greet(input: &serde_json::Value) -> ActionResult {
    let name = input["name"].as_str().unwrap_or("world");
    ok(
        serde_json::json!({ "greeting": format!("Hello, {name}!") }),
        "",
    )
}

/// The "echo" action — returns the input unchanged.
fn echo(input: &serde_json::Value) -> ActionResult {
    ok(input.clone(), "echoed input")
}

struct HelloConnector;

impl Guest for HelloConnector {
    /// Must agree with `[[actions]]` in `manifest.toml`. Both actions
    /// only compute from their input, so both are read-only.
    fn actions() -> Vec<ActionDecl> {
        vec![
            ActionDecl {
                name: "greet".to_owned(),
                description: "Returns a greeting for the given name".to_owned(),
                read_only: true,
            },
            ActionDecl {
                name: "echo".to_owned(),
                description: "Returns the input unchanged".to_owned(),
                read_only: true,
            },
        ]
    }

    fn execute(action: String, input: String) -> ActionResult {
        let parsed: serde_json::Value = match serde_json::from_str(&input) {
            Ok(value) => value,
            Err(e) => return err(format!("invalid input JSON: {e}")),
        };
        match action.as_str() {
            "greet" => greet(&parsed),
            "echo" => echo(&parsed),
            other => err(format!("unknown action: {other}")),
        }
    }
}

export!(HelloConnector);
