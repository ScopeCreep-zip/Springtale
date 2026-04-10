//! Host function imports — safe Rust wrappers for Springtale host APIs.
//!
//! These functions are provided by the Springtale runtime to WASM guests.
//! They're imported from the `"springtale"` module namespace and gated
//! by the connector's declared capabilities.

#[link(wasm_import_module = "springtale")]
unsafe extern "C" {
    /// Request HTTP access to a URL.
    ///
    /// Imported from the `"springtale"` WASM module namespace.
    /// The host checks the target host against the connector's declared
    /// `NetworkOutbound` capabilities before allowing the request.
    ///
    /// Returns:
    /// -  0 = allowed (request will be made)
    /// - -1 = invalid arguments (bad URL, out-of-bounds pointer)
    /// - -2 = capability denied (host not in NetworkOutbound allow-list)
    fn http_request(url_ptr: i32, url_len: i32, method_ptr: i32, method_len: i32) -> i32;
}

/// Check if an HTTP request to the given URL is allowed.
///
/// Returns `Ok(())` if the host approves, `Err(message)` if denied.
/// The host checks against `NetworkOutbound { host }` capabilities
/// declared in the connector's manifest.
pub fn check_http_access(url: &str, method: &str) -> Result<(), String> {
    let result = unsafe {
        http_request(
            url.as_ptr() as i32,
            url.len() as i32,
            method.as_ptr() as i32,
            method.len() as i32,
        )
    };

    match result {
        0 => Ok(()),
        -1 => Err("invalid request arguments".into()),
        -2 => Err("network access denied — host not in NetworkOutbound capabilities".into()),
        code => Err(format!("unknown host response: {code}")),
    }
}
