//! Make the embedded dashboard folder exist before the macro looks for it.
//!
//! `DashboardAssets` embeds `tauri/apps/dashboard/dist/`, and `rust_embed`
//! fails the build when that directory is absent. The directory is gitignored
//! apart from a placeholder, and a frontend build that empties its output
//! directory removes the placeholder along with the built files, which then
//! breaks every Rust build from a clean checkout. Creating it here makes the
//! backend independent of the frontend's build artefacts.

fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tauri/apps/dashboard/dist");
    if let Err(e) = std::fs::create_dir_all(&dist) {
        println!("cargo:warning=could not create {}: {e}", dist.display());
    }
    println!("cargo:rerun-if-changed=../../tauri/apps/dashboard/dist");
}
