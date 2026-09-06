# CLI Reference

`springtale` — command-line interface for managing connectors, rules, events, vault, formations, and the daemon.

```
   springtale
     │
     ├── init                         create vault + DB, optional onboarding
     ├── new TEMPLATE                 scaffold from starter template
     ├── server start                 run springtaled inline
     ├── run                          alias for `server start`
     ├── healthcheck [--url]          probe /health, exit 0 on healthy
     ├── doctor                       diagnostic checks
     ├── fix ERROR_ID                 apply an auto-repair for E001..E009
     ├── trace [--connector --rule]   real-time execution trace
     ├── panic                        emergency wipe (no confirm)
     │
     ├── connector { list, install PATH, sign PATH, enable NAME, disable NAME, remove NAME }
     ├── author    { add [NAME] [PUBKEY] --self, list, remove NAME }
     ├── rule      { list, add FILE, toggle ID, run ID, update ID FILE, delete ID }
     ├── events    [--limit N --connector NAME]
     ├── agent     set-autonomy RULE LEVEL
     ├── config    ai { get --scope, set --scope --type }
     │
     ├── vault     duress-setup
     ├── crypto    rotate-vault-key
     ├── bot       { pair-init, panic-unpair }
     │
     ├── travel    { prepare --backup-to, restore --from }
     ├── memory    { audit, compact --max-entries N }
     └── data      { export [--output --encrypt], import --input, purge }
```

*Fig. 1. CLI surface at a glance. `--json`, `--passphrase-file`, and `--passphrase-command` are global flags on every subcommand.*

## 1. Global Options

| Flag | Description |
|---|---|
| `--json` | Output as JSON instead of formatted tables |
| `--passphrase-file <path>` | Read the vault passphrase from a file. The file must be mode `0600`; anything more permissive is refused. |
| `--passphrase-command <cmd>` | Run `<cmd>` and use its stdout as the vault passphrase (for OS keychains, `pass`, secret managers). |

The store is always encrypted, so every store-backed command (connector, rule,
events, agent, author, config, memory, data, …) plus `doctor` and `fix` needs
the vault passphrase. With neither flag set the CLI prompts interactively on the
terminal. The passphrase never goes through argv or the environment.

---

## 2. `springtale init`

Create the data directory, vault, and database. Prompts for a passphrase interactively.

```
$ springtale init
Enter vault passphrase: ********
Confirm passphrase: ********
Vault created at ~/.local/share/springtale/vault.bin
Database created at ~/.local/share/springtale/springtale.db
```

---

## 3. `springtale server start`

Start the `springtaled` daemon inline (foreground). Useful for development.

```
$ springtale server start
INFO springtaled: listening on 127.0.0.1:8080
READY 8080
```

`springtaled` itself takes two flags used by the desktop shell, which runs
it as a sidecar rather than embedding a second copy of the runtime:

| Flag | Effect |
|------|--------|
| `--bind ADDR` | Override `[api] bind`. `127.0.0.1:0` binds a free port; the port actually bound is what the `READY {port}` line reports. |
| `--passphrase-stdin` | Read the vault passphrase as exactly one line from stdin, ahead of `SPRINGTALE_PASSPHRASE_FILE`, `SPRINGTALE_PASSPHRASE` and the TTY prompt. Keeps it out of `argv` and the environment, both readable by other local processes. |

`springtale run` is a plain-English alias for the same thing —
`springtale init cli-runner && springtale run` is the documented
zero-to-running path.

### 3.0.1 `springtale healthcheck [--url BASE_URL]`

Probe the daemon's `/health` endpoint and exit `0` on a 2xx response,
non-zero otherwise (3-second timeout). `--url` defaults to
`http://127.0.0.1:8080`, matching the `springtaled` default bind.

Built for container `HEALTHCHECK` — the distroless final image has no
`wget` or `curl`:

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s CMD ["/usr/local/bin/springtale", "healthcheck"]
```

---

## 3.1. `springtale new <template>`

Scaffold a project from a starter template. 14 templates ship; pick whichever is closest to what you need and edit from there. Definitions live in `crates/springtale-runtime/src/operations/templates.rs`.

**TABLE I. STARTER TEMPLATES**

| Template | Description |
|---|---|
| `telegram-bot` | Telegram bot with a `/start` welcome rule |
| `github-monitor` | GitHub webhook → Telegram push notifications |
| `cron-runner` | Scheduled task automation (no chat connector) |
| `llm-assistant` | AI-powered chat assistant (Ollama / OpenAI / Anthropic) |
| `blank-bot` | Empty skeleton for experts — no connectors, no rules |
| `cli-runner` | Headless CLI task runner — spawns a formation per task |
| `llm-swarm` | 3-agent LLM swarm (researcher / writer / critic) on a single prompt |
| `discord-bot` | Discord bot with a `!start` welcome rule |
| `matrix-bot` | Matrix / Element chatbot skeleton — ready for `connector-matrix` |
| `webhook-receiver` | HTTP webhook → cooperation formation fan-out |
| `file-watcher` | Filesystem event → cooperation formation |
| `research-assistant` | Multi-source research LLM swarm with cited output |
| `code-review-swarm` | Git diff → 3-agent code review (readability / correctness / security) |
| `meeting-summarizer` | Audio / transcript → structured summary LLM swarm |

```
$ springtale new telegram-bot
Scaffolded telegram-bot project.
  rules/     — starter rules
  config/    — connector config skeleton
Next: edit config/connector-telegram.toml with your bot token.
```

See [`../guide/templates.md`](../guide/templates.md) for worked walkthroughs and prerequisites per template.

---

## 3.2. `springtale doctor`

Run the same diagnostic checks exposed by `GET /diagnostics`. Reports
configuration, connectivity, capability, and schema issues with a
stable error id (`E001` through `E009`).

```
$ springtale doctor
✓ vault reachable
✗ E003: connector-telegram missing bot token
✓ rule engine loaded
```

## 3.3. `springtale fix <error-id>`

Apply the bundled auto-repair for a diagnostic error id. Same logic as
`POST /fixes/{id}/apply`.

```
$ springtale fix E003
Fixed E003 — connector-telegram config stub written.
```

## 3.4. `springtale trace`

Real-time execution trace — tails rule triggers, action dispatches, and
sentinel verdicts. Filter by connector or rule.

```
$ springtale trace --connector connector-telegram
TRIG connector-telegram.message_received → rule: weather-command
ACT  connector-presearch.search ok (410ms)
OUT  connector-telegram.send_message ok
```

---

## 4. `springtale connector`

### 4.1 `connector install <path>`

Install a connector from a TOML manifest file. Verifies the Ed25519 signature before registering.

```
$ springtale connector install ./connector-kick.toml
Installed: connector-kick v0.1.0
```

### 4.2 `connector list`

List all installed connectors.

```
$ springtale connector list
┌──────────────────────┬─────────┬─────────┐
│ NAME                 │ VERSION │ ENABLED │
├──────────────────────┼─────────┼─────────┤
│ connector-kick       │ 0.1.0   │ true    │
│ connector-telegram   │ 0.1.0   │ true    │
│ connector-github     │ 0.1.0   │ false   │
└──────────────────────┴─────────┴─────────┘
```

### 4.3 `connector enable <name>` / `disable <name>` / `remove <name>`

```
$ springtale connector enable connector-github
Enabled: connector-github

$ springtale connector disable connector-github
Disabled: connector-github

$ springtale connector remove connector-github
Removed: connector-github
```

### 4.4 `connector sign <manifest-path>`

Sign a connector manifest with the local vault identity. Manifest signatures are
required — `connector install` rejects an unsigned manifest or one signed by an
author that is not registered (see §4.5). Run `author add --self` once so your
own key is trusted, then sign every manifest you build.

```
$ springtale connector sign ./connector-kick.toml
Signed: ./connector-kick.toml (author: local)
```

---

## 4.5. `springtale author`

Trusted authors are the Ed25519 public keys a manifest signature may come from.

### 4.5.1 `author add [name] [pubkey] --self`

Register a trusted author. With `--self`, registers the local vault identity —
the key `connector sign` uses — under `name` (default `local`). Without
`--self`, `name` and `pubkey` are both required and register a third-party key.

```
$ springtale author add --self
Added author: local

$ springtale author add alice <ed25519-public-key>
Added author: alice
```

### 4.5.2 `author list`

```
$ springtale author list
┌───────┬──────────────────────────────────────────────┐
│ NAME  │ PUBLIC KEY                                   │
├───────┼──────────────────────────────────────────────┤
│ local │ …                                            │
└───────┴──────────────────────────────────────────────┘
```

### 4.5.3 `author remove <name>`

Manifests signed by a removed author fail verification on the next load.

---

## 5. `springtale rule`

### 5.1 `rule add <file>`

Add a rule from a TOML or JSON file.

```
$ springtale rule add ./rules/stream-announce.toml
Added: stream-announce (id: a1b2c3d4-...)
```

### 5.2 `rule list`

```
$ springtale rule list
┌──────────────────┬──────────┬─────────────────────┐
│ NAME             │ STATUS   │ TRIGGER             │
├──────────────────┼──────────┼─────────────────────┤
│ stream-announce  │ enabled  │ ConnectorEvent      │
│ daily-backup     │ enabled  │ Cron                │
│ pr-announce      │ disabled │ ConnectorEvent      │
└──────────────────┴──────────┴─────────────────────┘
```

### 5.3 `rule toggle <id>`

```
$ springtale rule toggle a1b2c3d4-...
Toggled: stream-announce → disabled
```

### 5.4 `rule update <id> <file>`

Replace a rule definition from a file.

### 5.5 `rule delete <id>`

Delete a rule permanently.

### 5.6 `rule run <id>`

Manually evaluate a rule against a synthetic trigger (dry-run).

```
$ springtale rule run a1b2c3d4-...
Rule: stream-announce
Matched: true
Actions: 1
```

---

## 6. `springtale events`

Query the event log.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--limit` | `u32` | 50 | Number of events to return |
| `--connector` | `String` | (all) | Filter by connector name |

```
$ springtale events --limit 10 --connector connector-kick
┌─────────────────────┬──────────────────┬──────────────────┐
│ TIMESTAMP           │ CONNECTOR        │ TRIGGER          │
├─────────────────────┼──────────────────┼──────────────────┤
│ 2026-04-10 14:22:01 │ connector-kick   │ stream_live      │
│ 2026-04-10 12:05:33 │ connector-kick   │ chat_message     │
└─────────────────────┴──────────────────┴──────────────────┘
```

---

## 7. `springtale agent`

### 7.1 `agent set-autonomy <rule> <level>`

`<rule>` is a rule name or a rule id; the level is stored against the rule id.
Autonomy levels: `observe`, `suggest`, `act-with-approval`, `act-autonomously`.
A rule with no explicit setting runs at `act-autonomously`.

```
$ springtale agent set-autonomy watcher observe
Set autonomy: watcher → observe
```

---

## 8. `springtale vault duress-setup`

Configure a secondary duress passphrase that unlocks a decoy vault under coercion. Both passphrases produce valid decryption paths; the vault file size is constant (131,152 bytes) regardless of which is in use.

```
$ springtale vault duress-setup
Enter real passphrase: ********
Enter duress passphrase: ********
Duress vault configured.
```

---

## 9. `springtale crypto rotate-vault-key`

Re-encrypt the vault with a new passphrase. The API bearer token changes as a side effect — the token is `HMAC-SHA256(passphrase, "springtale-api-token")`.

```
$ springtale crypto rotate-vault-key
Enter current passphrase: ********
Enter new passphrase: ********
Vault re-encrypted. Update API clients with the new token.
```

---

## 9.1. `springtale bot`

Bot pairing management. Covers code generation and emergency revocation
for users paired through chat connectors. No chat access is required to
revoke — `panic-unpair` works from a recovered terminal.

### 9.1.1 `bot pair-init`

Generate a pairing code for a new user. The code is displayed on the
terminal only — never in chat — so the operator must convey it out of
band.

```
$ springtale bot pair-init
Pairing code: 847-291-530  (valid 10 min)
```

### 9.1.2 `bot panic-unpair`

Revoke ALL paired users and invalidate every outstanding pairing code.
For emergencies — no chat access needed.

```
$ springtale bot panic-unpair
Revoked 4 paired users, invalidated 1 outstanding code.
```

---

## 10. `springtale travel`

Travel mode prepares Springtale for a border crossing or device inspection: encrypt a backup, wipe the local install, then restore at destination.

### 10.1 `travel prepare --backup-to <path>`

```
$ springtale travel prepare --backup-to ~/secure-backup.enc
Backup written: ~/secure-backup.enc (encrypted with vault passphrase)
Local vault + database wiped.
```

### 10.2 `travel restore --from <path>`

```
$ springtale travel restore --from ~/secure-backup.enc
Enter passphrase: ********
Restored: vault, database, config.
```

---

## 11. `springtale panic`

Emergency wipe. Vault key material is zeroed in memory, then the vault file is overwritten with random bytes, fsync'd, and unlinked. SQLite databases are VACUUM'd and overwritten. Completes in under 3 seconds on a 1 MB vault.

```
$ springtale panic
Wiping... done in 0.8s.
```

**Limitation:** on SSDs with wear levelling, residual ciphertext may survive in the flash translation layer. Full-disk encryption is the only complete mitigation — Springtale's panic wipe destroys the **key**, which is sufficient to make any residual data unreadable, but not to physically remove it from all blocks.

---

## 12. `springtale memory`

### 12.1 `memory audit`

List memory sessions and their entry counts.

```
$ springtale memory audit
┌──────────────────┬─────────┬─────────┐
│ SESSION          │ ENTRIES │ BYTES   │
├──────────────────┼─────────┼─────────┤
│ telegram:alice   │ 124     │ 56K     │
│ telegram:bob     │ 43      │ 12K     │
└──────────────────┴─────────┴─────────┘
```

### 12.2 `memory compact [--max-entries N]`

Delete oldest entries beyond the per-session cap (default 100).

---

## 13. `springtale data`

### 13.1 `data export [--output <path>] [--encrypt]`

Export all user data as JSON. With `--encrypt`, the output is encrypted with the vault passphrase.

### 13.2 `data import --input <path>`

Re-import a previously exported JSON archive. Replays rules, connector configs, and event history into the current store. The vault is untouched.

```
$ springtale data import --input backup.json
Imported: 12 rules, 4 connectors, 8421 events
```

If the input was produced with `data export --encrypt`, decrypt first or use `springtale travel restore --from <path>` instead.

> **CLI-only operation.** Import opens the SQLite backend directly via the local
> `SqliteBackend` and runs the runtime's `import_data()` function offline. There
> is **no** corresponding HTTP API endpoint — by design, because import is a
> destructive write to the store that must not race with the daemon's other
> writers. Stop the daemon (or use `--ephemeral` for the import session) before
> running this command.

### 13.3 `data purge`

Delete all user data (rules, events, memory, formations) without touching the vault.

---

## 14. `springtale config`

### 14.1 `config ai get --scope <scope>`

Show the AI adapter configured at one level. `--scope` is `colony`,
`formation <id>`, or `agent <rule-id>`. AI is per level — there is no global
adapter and no `[ai_*]` TOML section; the same settings are reachable from the
dashboard and `POST /config/ai/configure`.

```
$ springtale config ai get --scope agent 7c1e…
type: ollama
model: llama3.1:8b
base_url: http://127.0.0.1:11434
```

### 14.2 `config ai set --scope <scope> --type <type> [--model] [--base-url] [--api-key-stdin]`

`--type` is `noop`, `ollama`, `openai`, or `anthropic`. `--api-key-stdin` reads
the API key from stdin (never argv) and stores it in the vault.

```
$ springtale config ai set --scope colony --type ollama --model llama3.1:8b
AI adapter (colony): ollama / llama3.1:8b

$ springtale config ai set --scope formation 3 --type anthropic --model claude-sonnet-4-6 --api-key-stdin < key.txt
```

---

## References

- [1] Configuration: [configuration.md](configuration.md)
- [2] API endpoints: [api.md](api.md)
- [3] Rule authoring: [../guide/rules.md](../guide/rules.md)
- [4] Security model: [../arch/SECURITY.md](../arch/SECURITY.md)
