# Configuration Reference

Springtale is configured via a TOML file and optional environment variable overrides.

## 1. File Location

The daemon looks for `springtale.toml` in the current working directory. Override with the `SPRINGTALE_CONFIG` environment variable or `--config` flag.

## 2. Sections

**TABLE I. CONFIGURATION KEYS**

| Section | Key | Type | Default | Description |
|---------|-----|------|---------|-------------|
| `[store]` | `path` | `PathBuf` | `~/.local/share/springtale/springtale.db` | SQLite database file path |
| `[crypto]` | `vault_path` | `PathBuf` | `~/.local/share/springtale/vault.bin` | Encrypted vault file path |
| `[transport]` | `socket_path` | `PathBuf` | `~/.local/share/springtale/springtale.sock` | Unix domain socket path |
| `[api]` | `bind` | `String` | `"127.0.0.1:8080"` | API listen address (min 1 char) |
| `[api]` | `rate_limit_per_sec` | `u32` | `100` | Max requests per second (range: 1-10000) |

Full example:

```toml
[store]
path = "/home/user/.local/share/springtale/springtale.db"

[crypto]
vault_path = "/home/user/.local/share/springtale/vault.bin"

[transport]
socket_path = "/home/user/.local/share/springtale/springtale.sock"

[api]
bind = "127.0.0.1:8080"
rate_limit_per_sec = 100
```

## 3. Environment Overrides

Environment variables override file values. Prefix: `SPRINGTALE_`. Nesting separator: `__` (double underscore).

**TABLE II. ENVIRONMENT VARIABLE MAPPING**

| Variable | Overrides |
|----------|-----------|
| `SPRINGTALE_STORE__PATH` | `[store] path` |
| `SPRINGTALE_CRYPTO__VAULT_PATH` | `[crypto] vault_path` |
| `SPRINGTALE_TRANSPORT__SOCKET_PATH` | `[transport] socket_path` |
| `SPRINGTALE_API__BIND` | `[api] bind` |
| `SPRINGTALE_API__RATE_LIMIT_PER_SEC` | `[api] rate_limit_per_sec` |
| `SPRINGTALE_PASSPHRASE` | Vault passphrase (used by daemon and Docker) |
| `RUST_LOG` | Log level filter (e.g., `info`, `debug`, `springtaled=trace`) |

Priority (highest wins): environment variable → TOML file → built-in default.

## 4. Docker Environment

When running via Docker Compose, paths are mapped to the `/data` volume:

```yaml
environment:
  - SPRINGTALE_PASSPHRASE=${SPRINGTALE_PASSPHRASE}
  - SPRINGTALE_STORE__PATH=/data/springtale.db
  - SPRINGTALE_CRYPTO__VAULT_PATH=/data/vault.bin
  - SPRINGTALE_TRANSPORT__SOCKET_PATH=/data/springtale.sock
  - SPRINGTALE_API__BIND=0.0.0.0:8080
  - RUST_LOG=info
```

The Docker container mounts `./springtale.toml` read-only at `/etc/springtale/springtale.toml` and `./data` at `/data`.

## 5. Security Notes

- The API binds to `127.0.0.1` by default. Binding to `0.0.0.0` exposes the management API to the network — only do this behind a reverse proxy or in Docker.
- `SPRINGTALE_PASSPHRASE` should be set via a secret manager or `.env` file, not hardcoded in compose files.
- Database file permissions are set to `0o600` (owner read/write only) on creation.

---

## References

- [1] API endpoint reference: [api.md](api.md)
- [2] Docker deployment: [QUICKSTART.md](../QUICKSTART.md) §3
- [3] Config loading: `apps/springtaled/src/config.rs`
