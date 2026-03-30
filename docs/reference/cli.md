# CLI Reference

`springtale` — command-line interface for managing connectors, rules, events, and the daemon.

## 1. Global Options

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON instead of formatted tables |

---

## 2. `springtale init`

Create the data directory, vault, and database. Prompts for a passphrase interactively.

```bash
$ springtale init
Enter vault passphrase: ********
Confirm passphrase: ********
Vault created at ~/.local/share/springtale/vault.bin
Database created at ~/.local/share/springtale/springtale.db
```

---

## 3. `springtale server start`

Start the springtaled daemon inline (foreground). Useful for development.

```bash
$ springtale server start
INFO springtaled: listening on 127.0.0.1:8080
INFO springtaled: ready
```

---

## 4. `springtale connector`

### 4.1. `connector install <path>`

Install a connector from a TOML manifest file.

```bash
$ springtale connector install ./connector-kick.toml
Installed: connector-kick v0.1.0
```

### 4.2. `connector list`

List all installed connectors.

```bash
$ springtale connector list
┌──────────────────────┬─────────┬─────────┐
│ NAME                 │ VERSION │ ENABLED │
├──────────────────────┼─────────┼─────────┤
│ connector-kick       │ 0.1.0   │ true    │
│ connector-bluesky    │ 0.1.0   │ true    │
│ connector-github     │ 0.1.0   │ false   │
└──────────────────────┴─────────┴─────────┘
```

```bash
$ springtale connector list --json
{"connectors":[{"name":"connector-kick","version":"0.1.0","enabled":true},...]}
```

### 4.3. `connector enable <name>`

Enable a disabled connector.

```bash
$ springtale connector enable connector-github
Enabled: connector-github
```

### 4.4. `connector disable <name>`

Disable a connector without removing it.

```bash
$ springtale connector disable connector-github
Disabled: connector-github
```

### 4.5. `connector remove <name>`

Remove a connector and its registration.

```bash
$ springtale connector remove connector-github
Removed: connector-github
```

---

## 5. `springtale rule`

### 5.1. `rule add <file>`

Add a rule from a TOML or JSON file.

```bash
$ springtale rule add ./rules/stream-announce.toml
Added: stream-announce (id: a1b2c3d4-...)
```

### 5.2. `rule list`

List all rules with status.

```bash
$ springtale rule list
┌──────────────────┬──────────┬─────────────────────┐
│ NAME             │ STATUS   │ TRIGGER             │
├──────────────────┼──────────┼─────────────────────┤
│ stream-announce  │ enabled  │ ConnectorEvent      │
│ daily-backup     │ enabled  │ Cron                │
│ pr-announce      │ disabled │ ConnectorEvent      │
└──────────────────┴──────────┴─────────────────────┘
```

### 5.3. `rule toggle <id>`

Toggle a rule between enabled and disabled.

```bash
$ springtale rule toggle a1b2c3d4-...
Toggled: stream-announce → disabled
```

### 5.4. `rule run <id>`

Manually evaluate a rule (dry-run). Shows whether it would match and how many actions would fire.

```bash
$ springtale rule run a1b2c3d4-...
Rule: stream-announce
Matched: true
Actions: 1
```

---

## 6. `springtale events`

Query the event log.

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--limit` | `u32` | 50 | Number of events to return |
| `--connector` | `String` | (all) | Filter by connector name |

```bash
$ springtale events --limit 10 --connector connector-kick
┌─────────────────────┬──────────────────┬──────────────────┐
│ TIMESTAMP           │ CONNECTOR        │ TRIGGER          │
├─────────────────────┼──────────────────┼──────────────────┤
│ 2026-03-29 14:22:01 │ connector-kick   │ stream_live      │
│ 2026-03-29 12:05:33 │ connector-kick   │ chat_message     │
└─────────────────────┴──────────────────┴──────────────────┘
```

---

## References

- [1] Configuration: [configuration.md](configuration.md)
- [2] API endpoints: [api.md](api.md)
- [3] Rule authoring: [guide/rules.md](../guide/rules.md)
