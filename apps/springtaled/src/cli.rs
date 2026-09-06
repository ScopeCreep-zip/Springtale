//! Command-line interface for `springtaled`.
//!
//! Two flags exist purely so the desktop shell can run the daemon as a
//! Tauri sidecar (plan 2.1 — "springtaled is the only state owner"):
//! `--bind` lets the parent pick the loopback port (`127.0.0.1:0` for
//! "any free port"), and `--passphrase-stdin` hands the vault passphrase
//! over a pipe so it never appears in `argv` or the environment, where
//! any other local process could read it.

use clap::Parser;

/// Parsed `springtaled` command line.
#[derive(Parser, Debug, Clone)]
#[command(name = "springtaled", version, about = "Springtale daemon")]
pub struct Cli {
    /// Address the management API binds to.
    ///
    /// Overrides `[api] bind` in `springtale.toml`. `127.0.0.1:0` asks
    /// the OS for a free port; the port actually bound is printed on the
    /// `READY {port}` line.
    #[arg(long, value_name = "ADDR")]
    pub bind: Option<String>,

    /// Read the vault passphrase as exactly one line from stdin.
    ///
    /// Takes priority over `SPRINGTALE_PASSPHRASE_FILE`,
    /// `SPRINGTALE_PASSPHRASE` and the interactive TTY prompt.
    #[arg(long)]
    pub passphrase_stdin: bool,

    /// Print the OpenAPI document to stdout and exit.
    ///
    /// Derived from the handler annotations, so it needs no vault, no
    /// store and no bound port. CI regenerates it and diffs it against
    /// the copy the frontend generates its TypeScript from.
    #[arg(long)]
    pub dump_openapi: bool,
}

impl Cli {
    /// Parse the process arguments, exiting with clap's usage message on
    /// error (the standard `clap` behaviour for a binary entry point).
    #[must_use]
    pub fn from_args() -> Self {
        <Self as Parser>::parse()
    }

    /// Convert into the options the boot sequence consumes.
    #[must_use]
    pub fn into_boot_options(self) -> crate::runtime::BootOptions {
        crate::runtime::BootOptions {
            bind: self.bind,
            passphrase_stdin: self.passphrase_stdin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn test_cli_no_flags_leaves_config_in_charge() {
        let cli = Cli::parse_from(["springtaled"]);
        assert_eq!(cli.bind, None);
        assert!(!cli.passphrase_stdin);
    }

    #[test]
    fn test_cli_sidecar_flags_parse() {
        let cli = Cli::parse_from(["springtaled", "--bind", "127.0.0.1:0", "--passphrase-stdin"]);
        assert_eq!(cli.bind.as_deref(), Some("127.0.0.1:0"));
        assert!(cli.passphrase_stdin);
    }

    #[test]
    fn test_cli_into_boot_options_carries_both_flags() {
        let options = Cli::parse_from([
            "springtaled",
            "--bind",
            "0.0.0.0:9999",
            "--passphrase-stdin",
        ])
        .into_boot_options();
        assert_eq!(options.bind.as_deref(), Some("0.0.0.0:9999"));
        assert!(options.passphrase_stdin);
    }
}
