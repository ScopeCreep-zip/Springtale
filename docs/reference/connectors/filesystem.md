# connector-filesystem

Local filesystem integration. Watch directories for changes, read/write files, and list directories — all constrained to configured allow-list paths.

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `watch_paths` | `Vec<PathBuf>` | (required) | Directories to monitor for filesystem events |
| `read_paths` | `Vec<PathBuf>` | (required) | Allow-list for read operations |
| `write_paths` | `Vec<PathBuf>` | (required) | Allow-list for write operations |
| `debounce_ms` | `u64` | `500` | Debounce interval for file events (milliseconds) |

## 2. Authentication

None. Access is controlled by the path allow-lists in configuration.

All paths are canonicalized before checking — symlink traversal attacks are prevented. A request to read `/data/inbox/../../etc/passwd` resolves to `/etc/passwd`, which won't match any configured `read_paths`.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Description | Payload fields |
|------|-------------|---------------|
| `file_created` | A file was created in a watched directory | `path`, `event: "create"`, `filename`, `extension` |
| `file_modified` | A file was modified in a watched directory | `path`, `event: "modify"`, `filename`, `extension` |
| `file_deleted` | A file was deleted in a watched directory | `path`, `event: "delete"`, `filename`, `extension` |

Triggers use the `notify` crate with debouncing to coalesce rapid filesystem events. The debounce window is configurable (default 500ms).

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `read_file` | `path: String` | `content: String`, `size_bytes: u64` |
| `write_file` | `path: String`, `content: String`, `append: bool` (default: `false`) | `bytes_written: u64` |
| `list_dir` | `path: String` | `entries: [{name, path, is_dir, is_file, size_bytes}]`, `count: u64` |

## 5. Capabilities Required

Capabilities are generated dynamically from configuration:

| Capability | Parameter |
|-----------|-----------|
| `FilesystemRead` | Each path in `watch_paths` and `read_paths` |
| `FilesystemWrite` | Each path in `write_paths` |

## 6. Example Rule

```toml
[rule]
name = "auto-archive"

[trigger]
type = "ConnectorEvent"
connector = "connector-filesystem"
event = "file_created"

[[conditions]]
type = "Regex"
field = "trigger.extension"
pattern = "\\.(csv|json|xml)$"

[[actions]]
type = "WriteFile"
destination = "/data/archive/${trigger.filename}"
content = ""
delete_source = true
```
