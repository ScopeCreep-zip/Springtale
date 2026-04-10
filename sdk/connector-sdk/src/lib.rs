//! Springtale Connector SDK — build WASM connectors for the Springtale platform.
//!
//! This SDK handles the ABI contract between your connector code and the
//! Springtale host runtime. You write action handlers in Rust, the SDK
//! handles memory management, serialization, and host function imports.
//!
//! ## Quick Start
//!
//! ```rust
//! use springtale_connector_sdk::*;
//!
//! fn greet(input: serde_json::Value) -> ActionResult {
//!     let name = input["name"].as_str().unwrap_or("world");
//!     ActionResult::ok(serde_json::json!({"greeting": format!("Hello, {}!", name)}))
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn execute(ap: i32, al: i32, ip: i32, il: i32) -> i32 {
//!     dispatch(ap, al, ip, il, |action, input| match action {
//!         "greet" => greet(input),
//!         _ => ActionResult::error("unknown action"),
//!     })
//! }
//! ```
//!
//! Build: `cargo build --target wasm32-unknown-unknown --release`

pub mod host;
mod memory;

pub use serde_json;

/// Result of a connector action execution.
///
/// Must match `springtale_connector::connector::trait_::ActionResult` exactly.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub message: String,
}

impl ActionResult {
    /// Create a successful result with output data.
    pub fn ok(output: serde_json::Value) -> Self {
        Self {
            success: true,
            output,
            message: String::new(),
        }
    }

    /// Create a successful result with output and message.
    pub fn ok_with_message(output: serde_json::Value, message: impl Into<String>) -> Self {
        Self {
            success: true,
            output,
            message: message.into(),
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: serde_json::Value::Null,
            message: message.into(),
        }
    }
}

/// Dispatch an action call from the host.
///
/// This is the core ABI bridge. The host calls `execute(action_ptr, action_len,
/// input_ptr, input_len)` and this function:
/// 1. Reads the action name and input JSON from host-written memory
/// 2. Calls your handler function
/// 3. Serializes the ActionResult to JSON
/// 4. Writes the length-prefixed result to guest memory
/// 5. Returns the pointer to the result
///
/// ## ABI Contract
///
/// - Host writes action name at offset 1024
/// - Host writes input JSON at offset 1024 + action_len
/// - Guest returns pointer to: `[4 bytes: len (LE u32)][len bytes: JSON]`
pub fn dispatch(
    action_ptr: i32,
    action_len: i32,
    input_ptr: i32,
    input_len: i32,
    handler: impl FnOnce(&str, serde_json::Value) -> ActionResult,
) -> i32 {
    // Read action name from host-written memory
    let action = unsafe {
        let ptr = action_ptr as *const u8;
        let slice = core::slice::from_raw_parts(ptr, action_len as usize);
        match core::str::from_utf8(slice) {
            Ok(s) => s,
            Err(_) => {
                return write_result(&ActionResult::error("invalid UTF-8 in action name"));
            }
        }
    };

    // Read input JSON from host-written memory
    let input: serde_json::Value = unsafe {
        let ptr = input_ptr as *const u8;
        let slice = core::slice::from_raw_parts(ptr, input_len as usize);
        match serde_json::from_slice(slice) {
            Ok(v) => v,
            Err(e) => {
                return write_result(&ActionResult::error(format!("invalid input JSON: {e}")));
            }
        }
    };

    // Call the user's handler
    let result = handler(action, input);

    // Write result to guest memory and return pointer
    write_result(&result)
}

/// Serialize an ActionResult to JSON and write it to guest memory.
///
/// Returns the pointer to the length-prefixed result:
/// `[4 bytes: len (LE u32)][len bytes: JSON string]`
fn write_result(result: &ActionResult) -> i32 {
    let json = match serde_json::to_string(result) {
        Ok(s) => s,
        Err(_) => r#"{"success":false,"output":null,"message":"serialization failed"}"#.to_owned(),
    };

    let json_bytes = json.as_bytes();
    let total_len = 4 + json_bytes.len();

    // Allocate space in guest memory
    let ptr = memory::alloc(total_len);

    // Write length prefix (little-endian u32)
    let len_bytes = (json_bytes.len() as u32).to_le_bytes();
    unsafe {
        core::ptr::copy_nonoverlapping(len_bytes.as_ptr(), ptr as *mut u8, 4);
        core::ptr::copy_nonoverlapping(
            json_bytes.as_ptr(),
            (ptr as *mut u8).add(4),
            json_bytes.len(),
        );
    }

    ptr as i32
}
