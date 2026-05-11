//! pyo3's `extension-module` feature defers Python C-API symbol
//! resolution to runtime (the host Python provides `_Py_*` symbols when
//! it loads the produced `.so` / `.dylib`). On macOS, the default static
//! linker errors on unresolved symbols at link time; we need to pass
//! `-undefined dynamic_lookup` to skip that check. On Linux, GNU ld is
//! permissive by default so no flags are needed. On Windows, Python
//! distributes a `.lib` and pyo3-build-config handles linking via the
//! `python3.lib` it locates — no extra flags here either.
//!
//! Without this build script, `cargo build -p springtale-py` fails on
//! macOS with hundreds of `_Py_*` "symbol not found" errors at the
//! final dylib link step.

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        println!("cargo:rustc-link-arg=-undefined");
        println!("cargo:rustc-link-arg=dynamic_lookup");
    }
}
