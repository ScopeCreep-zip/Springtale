# Installing with Nix

The repo includes a `flake.nix` that defines a reproducible dev shell.
This is the recommended path for contributors who already use Nix and a
solid option for anyone who wants to avoid managing Rust toolchains
manually.

## Prerequisites

- Nix 2.18+ with flakes enabled. Add to `~/.config/nix/nix.conf`:
  ```
  experimental-features = nix-command flakes
  ```
- Optional but recommended: `direnv` + `nix-direnv`. Loads the dev
  shell automatically when you `cd` into the repo.

## Use the dev shell

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
nix develop          # or `direnv allow` if you have direnv set up
```

You're now in a shell with:

- The pinned Rust toolchain (matches `rust-toolchain.toml`).
- `cargo-nextest`, `cargo-deny`, `cargo-audit`, `cargo-watch`.
- `pnpm` and Node for the Tauri frontend.
- `pkg-config`, OpenSSL headers, and the C compiler for the build.
- `wasm-tools` and `wasmtime` CLI for inspecting WASM connectors.

Inside the shell, the standard commands all work:

```bash
cargo build --workspace
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Exit the shell with `exit` or Ctrl-D and your environment is exactly
how you found it.

## Konductor integration

[Konductor](https://github.com/braincraftio/konductor) is the Nix
framework Springtale draws from. If you're already running a Konductor
multi-repo setup, Springtale slots in as one of the projects.

## Building a Nix package

```bash
nix build .#springtaled
nix build .#springtale-cli
```

Both produce a closure under `./result/bin/`. Symlink onto your `PATH`
or include them in a `home-manager` / NixOS configuration.

A minimal NixOS module looks like this:

```nix
{ pkgs, ... }:
{
  systemd.services.springtaled = {
    description = "Springtale daemon";
    after = [ "network.target" ];
    wantedBy = [ "multi-user.target" ];

    serviceConfig = {
      ExecStart = "${pkgs.springtaled}/bin/springtaled";
      Restart = "on-failure";
      User = "springtale";
      Group = "springtale";

      # Hardening — see docs/installation/systemd.md for the full list.
      NoNewPrivileges = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      PrivateTmp = true;
      PrivateDevices = true;
      ReadWritePaths = [ "/var/lib/springtale" ];

      Environment = [
        "SPRINGTALE_PASSPHRASE_FILE=/run/secrets/springtale-passphrase"
        "SPRINGTALE_DATA_DIR=/var/lib/springtale"
      ];
    };
  };

  users.users.springtale = {
    isSystemUser = true;
    group = "springtale";
    home = "/var/lib/springtale";
    createHome = true;
  };
  users.groups.springtale = {};
}
```

Pair with `sops-nix` or `agenix` for the passphrase secret.

## Cross-compiling

The flake defines `springtaled-static` for x86_64-linux-musl static
builds. Useful for shipping a single binary to a server without a
matching glibc.

```bash
nix build .#springtaled-static
```

Output is a fully-static ELF; no dynamic linker needed.

## Cache hits

The flake includes a `cachix` configuration. If you want pre-built
dependencies (saves the 5–15 minute first build), enable the cache:

```bash
cachix use springtale     # if/when we publish a cache
```

We don't currently publish a cache — local Nix store sharing is
sufficient for the contributor base. If we ship binaries on a release
schedule, that's the point we'd publish.
