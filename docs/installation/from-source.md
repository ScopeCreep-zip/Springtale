# Installing from source

The default for contributors and anyone who wants to audit what they're
running.

## Prerequisites

- **Rust** 1.85 or newer. Use `rustup` rather than your distro's Rust;
  the workspace pins specific toolchain features. `rust-toolchain.toml`
  in the repo root sets the channel automatically when you `cd` in.
- **A C compiler** + **pkg-config** + **OpenSSL headers** — needed
  transitively by some dependencies' build scripts (not by Springtale
  itself; we don't link OpenSSL at runtime).
  - Debian/Ubuntu: `sudo apt install build-essential pkg-config libssl-dev`
  - Fedora: `sudo dnf install gcc pkgconfig openssl-devel`
  - macOS: `xcode-select --install` then `brew install pkg-config openssl`
- **git** — to clone the repo.
- Optional: **`cargo-nextest`** for the fast test runner (`cargo install cargo-nextest`).
- Optional: **`pnpm`** if you want to build the Tauri desktop or web dashboard.

## Clone and build

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
cargo build --release --workspace
```

First build is slow — Wasmtime, rustls, axum, and a few other heavy
deps need to compile. Expect 5–15 minutes on a reasonable laptop.
Incremental rebuilds after that are seconds.

After the build, the daemon is at `target/release/springtaled` and the
CLI is at `target/release/springtale-cli`. You can either:

- Use them directly: `./target/release/springtaled` and
  `./target/release/springtale-cli`.
- Copy them onto your `PATH`: `sudo cp target/release/{springtaled,springtale-cli} /usr/local/bin/`.
- Or use `cargo run --bin springtaled` and `cargo run --bin springtale-cli`
  while iterating (the convention used in the QUICKSTART).

## Run for the first time

```bash
springtale-cli init           # creates vault + database, prompts for passphrase
springtale-cli server start   # starts daemon on 127.0.0.1:8080
```

In another terminal:

```bash
springtale-cli connector list
springtale-cli rule list
```

If `init` fails with `E001` or similar, run `springtale-cli fix E001`
for the recovery runbook.

## Building the Tauri desktop (optional)

```bash
cd tauri/apps/desktop
pnpm install
pnpm tauri dev      # for development
pnpm tauri build    # for a release bundle
```

The bundle lands in `tauri/apps/desktop/src-tauri/target/release/bundle/`
in your platform's native format (.dmg / .deb / .msi / .AppImage).

## Building the web dashboard (optional)

```bash
cd tauri/apps/dashboard
pnpm install
pnpm dev      # dev server with HMR
pnpm build    # static bundle served by springtaled
```

The build output lands in `tauri/apps/dashboard/dist/`, which `springtaled`
serves at `http://127.0.0.1:8080/dashboard` once the daemon is running.

## Common build issues

| Error | Cause | Fix |
|---|---|---|
| `linking with cc failed: pkg-config not found` | Missing pkg-config | Install pkg-config via your package manager |
| `error: failed to run custom build command for openssl-sys` | Missing OpenSSL headers (one of our transitive deps wants them at build-time only) | Install `libssl-dev` / `openssl-devel` |
| `error: linker cc not found` | No C compiler | Install build-essential / xcode-select |
| `error: package native-tls cannot be built` | Something is trying to pull native-tls; we ban this. | Run `cargo tree -i native-tls`. The output will show what's pulling it. File a bug if it's a first-party crate; otherwise it's a transitive dep that needs feature-flag work. |
| Out of memory during link | wasmtime + rustls together can need ~6 GB peak for the linker | Add swap, or build with `CARGO_BUILD_JOBS=1` to serialise link steps |
| Builds work but tests fail with "Cannot allocate memory" | nextest spawns parallel test processes | Set `--test-threads=2` or use `cargo test --workspace` instead |

## Development environment

If you'll be iterating, set up:

- `cargo-watch` — `cargo install cargo-watch`, then `cargo watch -x check` keeps a hot type-check running.
- `bacon` — alternative continuous-build tool, nicer terminal UX.
- `cargo-deny` — `cargo install cargo-deny`, then `cargo deny check` enforces our dependency policy locally.
- `cargo-audit` — `cargo install cargo-audit`, then `cargo audit` runs the CVE check we run in CI.

The `flake.nix` provides all of these plus Rust itself in one direnv-loaded
shell — see [`nix.md`](nix.md) if you want that path.
