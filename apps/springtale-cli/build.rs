//! Build-time generation of shell completions and the `springtale(1)` man page.
//!
//! # Mechanism
//!
//! A build script cannot `use` items from the binary crate it builds, so the
//! clap definition has to reach this file some other way. `src/cli.rs` is
//! deliberately standalone — it imports nothing but `std::path::PathBuf` and
//! `clap` — so it is `include!`d here into a private `cli` module. The binary
//! keeps `mod cli;` unchanged; both compile the same source, and the
//! `#[cfg(test)]` block inside it is compiled out for the build script.
//!
//! If `src/cli.rs` ever grows a `crate::`/`super::` reference the include stops
//! compiling, and the fix is to move that reference out of `cli.rs` rather than
//! to weaken this script — the CLI surface is meant to be declarable on its own.
//!
//! # Output
//!
//! Everything lands under `$OUT_DIR/assets/`:
//!
//! ```text
//! assets/completions/springtale.bash
//! assets/completions/_springtale          (zsh)
//! assets/completions/springtale.fish
//! assets/completions/_springtale.ps1      (powershell)
//! assets/completions/springtale.elv       (elvish)
//! assets/man/springtale.1
//! ```
//!
//! `$OUT_DIR` is buried under `target/`, so the absolute path is also exported
//! as the `SPRINGTALE_ASSETS_DIR` compile-time env var for packaging scripts to
//! read back with `cargo build --message-format=json`. Setting the
//! `SPRINGTALE_ASSET_DIR` environment variable at build time mirrors every file
//! into that directory as well (used by the release packaging job).

use std::io::Result;
use std::path::{Path, PathBuf};

use clap::CommandFactory;
use clap_complete::Shell;

mod cli {
    include!("src/cli.rs");
}

fn main() -> Result<()> {
    println!("cargo::rerun-if-changed=src/cli.rs");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=SPRINGTALE_ASSET_DIR");

    let Some(out_dir) = std::env::var_os("OUT_DIR") else {
        // Not running under cargo (rust-analyzer probes, doc tooling).
        return Ok(());
    };
    let assets = PathBuf::from(out_dir).join("assets");
    generate_into(&assets)?;

    if let Some(extra) = std::env::var_os("SPRINGTALE_ASSET_DIR") {
        generate_into(Path::new(&extra))?;
    }

    println!(
        "cargo::rustc-env=SPRINGTALE_ASSETS_DIR={}",
        assets.display()
    );
    Ok(())
}

/// Write every completion script and the man page under `root`.
fn generate_into(root: &Path) -> Result<()> {
    let completions = root.join("completions");
    let man = root.join("man");
    std::fs::create_dir_all(&completions)?;
    std::fs::create_dir_all(&man)?;

    let mut command = cli::Cli::command();
    command.build();

    for shell in [
        Shell::Bash,
        Shell::Zsh,
        Shell::Fish,
        Shell::PowerShell,
        Shell::Elvish,
    ] {
        clap_complete::generate_to(shell, &mut command, "springtale", &completions)?;
    }

    let rendered = {
        let mut buf = Vec::new();
        clap_mangen::Man::new(command).render(&mut buf)?;
        buf
    };
    std::fs::write(man.join("springtale.1"), rendered)
}
