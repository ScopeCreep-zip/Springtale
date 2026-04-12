# connector-shell

Local shell command execution. Runs commands directly (not through a shell interpreter) with an allow-list, configurable timeout, and optional working directory.

```
   Rule engine                     connector-shell                      Shell
        │                                 │                               │
        │ execute(execute_command,        │                               │
        │         {command, args})        │                               │
        ├────────────────────────────────►│                               │
        │                                 │ capability gate: ShellExec    │
        │                                 │                               │
        │                                 │ allow-list check              │
        │                                 │ (allowed_commands)            │
        │                                 │                               │
        │                                 │ spawn with timeout_secs       │
        │                                 ├──────────────────────────────►│
        │                                 │                               │
        │                                 │ stdout + stderr + exit code   │
        │ ActionResult                    │ ◄─────────────────────────────┤
        │ ◄───────────────────────────────┤                               │
```

*Fig. 1. Shell data flow. Every command is gated on the `ShellExec` capability (which requires explicit user approval at install time) and checked against `allowed_commands` before being spawned.*

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `allowed_commands` | `Vec<String>` | (required) | Command names permitted for execution |
| `timeout_secs` | `u64` | `30` | Maximum execution time in seconds |
| `working_directory` | `Option<String>` | `None` | Working directory for command execution |

## 2. Authentication

None. Security is enforced by the command allow-list.

## 3. Triggers

None. This is an action-only connector.

## 4. Actions

**TABLE II. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `exec` | `command: String`, `args: Vec<String>` (default: `[]`) | `exit_code: i32`, `stdout: String`, `stderr: String` |

## 5. Capabilities Required

| Capability | Parameter |
|-----------|-----------|
| `ShellExec` | (none) |

`ShellExec` always requires explicit user approval. It cannot be auto-approved.

## 6. Example Rule

```toml
[rule]
name = "daily-backup"

[trigger]
type = "Cron"
expression = "0 3 * * *"

[[actions]]
type = "RunConnector"
connector = "connector-shell"
action = "exec"

[actions.params]
command = "backup-script"
args = ["--compress", "--destination", "/backups"]
```

## 7. Security Notes

- **Direct execution**: Commands are executed directly via `tokio::process::Command`, NOT through a shell interpreter (`sh -c`). Shell metacharacters, pipes, and redirects are not interpreted.
- **Allow-list enforcement**: The `command` field must match an entry in `allowed_commands`. A request to run `rm` when only `backup-script` is allowed is rejected.
- **Timeout**: Commands exceeding `timeout_secs` are killed via `tokio::time::timeout`.
- **Sandbox validation**: Input is validated to prevent shell metacharacter injection.
