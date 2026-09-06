# Quickstart

Get Springtale running and create your first automation in under 5 minutes.

## 1. Prerequisites

**TABLE I. REQUIRED TOOLS**

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | stable 1.85+ | Build the workspace (edition 2024) |
| Git | any | Clone the repo |

**TABLE II. OPTIONAL TOOLS**

| Tool | Purpose |
|------|---------|
| Nix + direnv | Reproducible dev shell via Konductor (includes all tools) |
| Docker | Container deployment |
| cargo-nextest | Faster parallel test runner |

---

## 2. Build from Source

```
  ┌───────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐     ┌───────────┐
  │ git clone │────>│ cargo build  │────>│ springtale   │────>│ springtale   │────>│ curl      │
  │           │     │ --workspace  │     │ init         │     │ server start │     │ /health   │
  └───────────┘     └──────────────┘     └──────────────┘     └──────────────┘     └───────────┘
```

*Fig. 1. Build and init flow.*

### 2.1. Clone and Build

```bash
git clone https://github.com/ScopeCreep-zip/Springtale.git
cd Springtale
cargo build --workspace
```

### 2.2. Initialize Vault

This creates the encrypted vault and SQLite database. You'll be prompted for a passphrase — this protects your stored credentials.

```bash
cargo run --bin springtale-cli -- init
```

### 2.3. Start the Daemon

```bash
cargo run --bin springtale-cli -- server start
```

### 2.4. Verify Health

In another terminal:

```bash
curl http://127.0.0.1:8080/health
# {"status": "ok"}

curl http://127.0.0.1:8080/ready
# {"status": "ready"}
```

Or use the built-in probe (`exit 0` = healthy) — this is what the
Docker image's `HEALTHCHECK` runs, since the distroless container has
no `curl`:

```bash
springtale healthcheck
```

---

## 3. Docker

### 3.1. Build Image

```bash
docker build -t springtale .
```

### 3.2. Run

The passphrase goes in a **file**, never in the environment. `docker-compose.yml`
mounts it as a Docker secret and points `SPRINGTALE_PASSPHRASE_FILE` at the
mount, which is the first source `get_passphrase()` consults at boot.

```bash
mkdir -p data secrets
cp springtale.toml.example springtale.toml

# Write the passphrase to the secret file. `-n` matters: no trailing
# newline. Leading `space` keeps it out of shell history on bash/zsh
# with HISTCONTROL=ignorespace / setopt HIST_IGNORE_SPACE.
 printf '%s' 'your-secure-passphrase' > secrets/passphrase.txt
chmod 600 secrets/passphrase.txt

# Create the vault before the first `up`
docker compose run --rm springtaled springtale init

docker compose up -d
```

Do **not** use `export SPRINGTALE_PASSPHRASE=...`. Any process running as
the same user can read another process's environment out of
`/proc/<pid>/environ`, `docker inspect` prints it back in the clear, and it
is inherited by every child process. `SPRINGTALE_PASSPHRASE` still exists as
a development-only fallback; production and anything holding real user data
should use the file.

The container runs as a non-root user with read-only root filesystem, all capabilities dropped, and `no-new-privileges` enforced.

**TABLE III. DOCKER ENVIRONMENT VARIABLES**

| Variable | Default | Description |
|----------|---------|-------------|
| `SPRINGTALE_PASSPHRASE_FILE` | `/run/secrets/springtale_passphrase` | Path to the file holding the vault passphrase. Read as bytes, trailing whitespace trimmed, zeroized after use. **Preferred.** |
| `SPRINGTALE_PASSPHRASE` | — | Vault passphrase inline. Development only — visible in `/proc/<pid>/environ` and `docker inspect`. Consulted only if `SPRINGTALE_PASSPHRASE_FILE` is unset |
| `SPRINGTALE_STORE__PATH` | `/data/springtale.db` | Database path inside container |
| `SPRINGTALE_CRYPTO__VAULT_PATH` | `/data/vault.bin` | Vault path inside container |
| `SPRINGTALE_TRANSPORT__SOCKET_PATH` | `/data/springtale.sock` | Control socket path inside container |
| `SPRINGTALE_API__BIND` | `0.0.0.0:8080` | API bind address |
| `RUST_LOG` | `info` | Log level |

> `secrets/passphrase.txt` is the one file that must never be committed or
> included in a backup image. Keep it out of the build context.

### 3.3. Verify

```bash
curl http://localhost:8080/health
# {"status": "ok"}
```

---

## 4. Nix Development Environment

If you have Nix and direnv installed:

```bash
cd Springtale
direnv allow
```

This loads the Konductor dev shell with Rust, security tooling (cargo-deny, cargo-audit, gitleaks, trivy), WASM tools (wabt), and Node.js 22.

---

## 5. Your First Automation

Let's create a rule that watches a directory and runs a shell command when a file appears.

### 5.1. Create a Watch Directory

```bash
mkdir -p /tmp/springtale-inbox
```

### 5.2. Write a Rule

Create `rules/file-alert.toml`:

```toml
[rule]
name = "file-alert"

[trigger]
type = "FileWatch"
path = "/tmp/springtale-inbox"
event = "create"

[[actions]]
type = "SendMessage"
text = "New file detected: ${trigger.filename}"
```

### 5.3. Add the Rule

```bash
cargo run --bin springtale-cli -- rule add rules/file-alert.toml
# Added: file-alert (id: ...)
```

### 5.4. Verify It's Listed

```bash
cargo run --bin springtale-cli -- rule list
```

### 5.5. Test It

With the daemon running, create a file in the watched directory:

```bash
touch /tmp/springtale-inbox/hello.txt
```

Check the event log:

```bash
cargo run --bin springtale-cli -- events --limit 5
```

```
  What happened:

  ┌─────────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────┐
  │ File created in │────>│ FileWatch    │────>│ Rule engine  │────>│ Action:  │
  │ /tmp/springtale-│     │ trigger      │     │ matches      │     │ Send     │
  │ inbox/          │     │ fires        │     │ "file-alert" │     │ Message  │
  └─────────────────┘     └──────────────┘     └──────────────┘     └──────────┘
```

*Fig. 2. Data flow for the file alert rule.*

---

## 6. Next Steps

| I want to... | Read |
|---|---|
| Understand how the pieces fit together | [guide/architecture.md](guide/architecture.md) |
| Learn about security and privacy | [guide/security.md](guide/security.md) |
| Explore available connectors | [guide/connectors.md](guide/connectors.md) |
| Write more complex rules | [guide/rules.md](guide/rules.md) |
| Look up CLI commands | [reference/cli.md](reference/cli.md) |
| Look up API endpoints | [reference/api.md](reference/api.md) |
| Build a new connector | [contributing/adding-a-connector.md](contributing/adding-a-connector.md) |
| See what's coming next | [ROADMAP.md](ROADMAP.md) |

---

## References

- [1] Configuration reference: [reference/configuration.md](reference/configuration.md)
- [2] CLI reference: [reference/cli.md](reference/cli.md)
- [3] Docker compose file: `docker-compose.yml`
