# connector-bluesky

Bluesky/ATProto integration. Session-based authentication, Jetstream firehose for real-time events, and post/reply/like/repost actions.

## 1. Configuration

**TABLE I. CONFIG FIELDS**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `identifier` | `String` | (required) | Bluesky handle (e.g., `user.bsky.social`) or DID (e.g., `did:plc:...`) |
| `password` | `Secret<String>` | (required) | Account password (app password recommended) |
| `pds_base` | `String` | `"https://bsky.social"` | Personal Data Server base URL |
| `jetstream_url` | `String` | `"wss://jetstream2.us-west.bsky.network/subscribe"` | Jetstream WebSocket URL |

## 2. Authentication

ATProto session authentication. The connector creates a session with the PDS using the identifier and password, receiving access and refresh tokens. Sessions are automatically refreshed.

App passwords are recommended over account passwords — they can be revoked independently and don't grant full account access.

## 3. Triggers

**TABLE II. TRIGGERS**

| Name | Source | Payload fields |
|------|--------|---------------|
| `mention` | Jetstream (filtered `app.bsky.feed.post`) | `did`, `uri`, `cid`, `text`, `facets`, `createdAt` |
| `follow` | Jetstream (`app.bsky.graph.follow`) | `did`, `uri`, `subject`, `createdAt` |
| `like` | Jetstream (`app.bsky.feed.like`) | `did`, `subject` (`uri`, `cid`), `createdAt` |
| `repost` | Jetstream (`app.bsky.feed.repost`) | `did`, `subject` (`uri`, `cid`), `createdAt` |

All triggers are delivered via the Jetstream WebSocket firehose — a real-time stream of ATProto events.

## 4. Actions

**TABLE III. ACTIONS**

| Name | Input fields | Output fields |
|------|-------------|--------------|
| `create_post` | `text: String` | `uri: String`, `cid: String`, `response: Object` |
| `reply` | `text`, `parent_uri`, `parent_cid`, `root_uri`, `root_cid` | `uri: String`, `cid: String`, `response: Object` |
| `like` | `subject_uri: String`, `subject_cid: String` | `uri: String`, `response: Object` |
| `repost` | `subject_uri: String`, `subject_cid: String` | `uri: String`, `response: Object` |

## 5. Capabilities Required

| Capability | Parameter |
|-----------|-----------|
| `NetworkOutbound` | `bsky.social` |
| `NetworkOutbound` | `jetstream2.us-west.bsky.network` |

## 6. Example Rule

```toml
[rule]
name = "auto-like-mentions"

[trigger]
type = "ConnectorEvent"
connector = "connector-bluesky"
event = "mention"

[[actions]]
type = "RunConnector"
connector = "connector-bluesky"
action = "like"

[actions.params]
subject_uri = "${trigger.uri}"
subject_cid = "${trigger.cid}"
```
