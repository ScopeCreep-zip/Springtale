//! Hello World WASM connector for Springtale.
//!
//! Demonstrates the minimum viable WASM connector:
//! - One action ("greet") that returns a greeting
//! - Proper ABI contract with the Springtale host
//!
//! Build: cargo build --target wasm32-unknown-unknown --release
//! Install: copy target/.../connector_hello_wasm.wasm + manifest.toml
//!          to Springtale and call install_wasm_connector()

use springtale_connector_sdk::{dispatch, ActionResult};

/// The "greet" action — takes a name, returns a greeting.
fn greet(input: serde_json::Value) -> ActionResult {
    let name = input["name"].as_str().unwrap_or("world");
    ActionResult::ok(serde_json::json!({
        "greeting": format!("Hello, {}!", name),
    }))
}

/// The "echo" action — returns the input unchanged.
fn echo(input: serde_json::Value) -> ActionResult {
    ActionResult::ok_with_message(input.clone(), "echoed input")
}

/// WASM entry point — dispatches action calls from the Springtale host.
///
/// The host writes action name at memory offset 1024 and input JSON
/// at 1024 + action_len. This function reads them, dispatches to the
/// correct handler, and returns a pointer to the JSON result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn execute(
    action_ptr: i32,
    action_len: i32,
    input_ptr: i32,
    input_len: i32,
) -> i32 {
    dispatch(action_ptr, action_len, input_ptr, input_len, |action, input| {
        match action {
            "greet" => greet(input),
            "echo" => echo(input),
            _ => ActionResult::error(format!("unknown action: {action}")),
        }
    })
}
