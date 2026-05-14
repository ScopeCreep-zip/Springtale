# External workspaces (D1)

When a formation handles chat messages, it eventually needs to know
*where it lives* — which Telegram chats, which Discord channels,
which Signal groups it's been invited to. That registry is the
**external workspace directory**.

External workspaces are an extension of the formation's mental
model: per-formation, gossip-replicated across members, populated
automatically as messages arrive. Nothing about a chat is stored
until the connector emits an event from it — there's no enumerable
discovery, no list-all-my-chats sweep.

This is D1 in the cooperation-alignment roadmap. The mental-model
spec ([`docs/intended-arch/COOPERATION.md §21`](../intended-arch/COOPERATION.md))
is the design parent; this page is the user-facing tour.

## The shape of a workspace

```
┌─────────────────────────────────────────────────────────────┐
│  MentalModelWorkspaceRow                                    │
│                                                             │
│   formation_id    "f47ac10b-58cc-…"                         │
│   workspace_key   "telegram://chat/12345"                   │
│   display_name    "Mutual aid coordination"                 │
│   kind            "supergroup"                              │
│   metadata_json   {"member_count": 47}                      │
│   first_seen_at   2026-05-08T14:21Z                         │
│   last_seen_at    2026-05-13T09:03Z                         │
└─────────────────────────────────────────────────────────────┘
```

Source: `crates/springtale-store/src/schema/sql/mental_model_workspaces.sql`.
Live type: `MentalModelWorkspaceRow` in `springtale-store`.

### `workspace_key` URI shape

Every workspace is addressed by a URI-shaped string. Each connector
owns its scheme (mapped 1:1 to the connector's name without the
`connector-` prefix):

```
telegram://chat/{chat_id}
telegram://channel/{username}
discord://guild/{guild_id}/channel/{channel_id}
discord://dm/{channel_id}
slack://channel/{channel_id}
slack://im/{user_id}
signal://group/{group_id}
signal://user/{phone_number}
irc://network/{network}/channel/{channel_name}
irc://network/{network}/user/{nick}
nostr://pubkey/{pubkey_hex}
bluesky://account/{did}
```

Source of truth: `crates/springtale-connector/src/workspace_key.rs`.

The cooperation crate's `WorkspaceKey(String)` newtype treats the
URI opaquely — only the connector layer parses it. No external
URL-parsing crate; the format is constrained enough to handle
in-house.

Connectors accept **either** a raw destination id (`"12345"`) **or**
a full URI (`"telegram://chat/12345"`) in their `send_message`
action. `workspace_key::extract_id_for_scheme()` does the at-boundary
translation so older hand-written recipe TOML keeps working.

## How workspaces get populated

```
   1. Connector emits a ConnectorEvent
                        │
                        ▼
   2. The Universal Mention Harvester (runtime)
      calls the connector's MentionExtractor::extract_destinations()
                        │
                        ▼
   3. Connector returns Vec<HarvestedDestination>
      { workspace_key, display_name, kind, metadata_json }
                        │
                        ▼
   4. Harvester upserts into mental_model_workspaces
      keyed by (formation_id, workspace_key)
                        │
                        ▼
   5. Gossip-delta merge replicates the new row to other formation
      members via the existing chitchat substrate
```

The harvester runs on **every** dispatched event without the
connector having to opt in. The only thing each connector
contributes is its `MentionExtractor` implementation — a pure
function from event payload to destination list.

### `MentionExtractor` trait

```rust
// crates/springtale-connector/src/mention.rs
pub trait MentionExtractor: Send + Sync {
    fn extract_destinations(&self, event: &Value) -> Vec<HarvestedDestination>;
}

pub struct HarvestedDestination {
    pub workspace_key:  String,    // URI-shaped, connector's scheme
    pub display_name:   String,    // chat title / channel name / user handle
    pub kind:           String,    // "user" | "group" | "channel" | "supergroup" | "dm" | "account" | "thread"
    pub metadata_json:  Option<Value>,  // member count, username, etc. — never roster
}
```

Connectors implement this themselves because every messaging
platform has a different event payload shape:

| Connector | Event field harvested |
|---|---|
| telegram | `message.chat.id`, `message.chat.title` |
| discord | `channel_id`, `guild_id`, names via cached lookup |
| slack | `channel` field, `team` for workspace scoping |
| signal | `groupId` / sender's `e164` |
| irc | network + channel name from PRIVMSG target |
| nostr | event's `pubkey` for DMs, hashtag for public posts |
| bluesky | actor `did` from likes/mentions/reposts |

Connectors that don't emit chat-like events (cron, filesystem,
HTTP-only) skip implementing `MentionExtractor`; the harvester
treats `None` as "no destinations to harvest" — no error.

## Privacy posture

Stricter than typical messaging integrations:

- **`display_name` is the only human-readable text stored.** Chat
  titles, channel names, user handles. **Never** message bodies,
  never member rosters, never timestamps of individual messages.
- **`metadata_json` may carry a member count** (an integer, useful
  for triage UI) **but never a member list**.
- **`first_seen_at` / `last_seen_at` are dispatch timestamps**, not
  message timestamps. They tell you "we've heard from this chat
  recently" — not what was said.

This matches the executions-log privacy posture (sizes only, never
content) and the broader Springtale rule: anything that would name
a user's social graph isn't persisted.

## Scoping

```
                  WORKSPACES ARE FORMATION-SCOPED
                              │
            ┌─────────────────┴─────────────────┐
            │                                   │
   Formation A's mental_model         Formation B's mental_model
   ─────────────────────────         ─────────────────────────
   telegram://chat/12345              telegram://chat/12345
   (display_name: "mutual aid")       (NOT VISIBLE in A's view)
   discord://guild/G1/channel/C       slack://channel/dev
```

A user with two formations sees **two independent directories**.
Even when both formations share the same underlying connector and
the same chat, each formation's mental model records the destination
separately. This is intentional — a formation's intent ("watch
GitHub for harassment patterns") may want to know about a chat that
another formation's intent ("post stream alerts") shouldn't.

Cascading delete: dissolving a formation drops its workspace rows
automatically via `DELETE CASCADE` on `formations(id)`.

## Within-formation gossip

`mental_model_workspaces` is gossip-replicated across formation
members via the chitchat substrate (see
[`docs/guide/cross-formation.md`](cross-formation.md) for the
gossip layer's mechanics). A destination learned by agent A in
formation F is visible to agent B in formation F automatically.

Conflict resolution happens in
`crates/springtale-cooperation/src/mental_model/external_workspaces.rs::merge_gossip_delta`:

- Same key, different display_name → last-writer-wins by
  `last_seen_at`. UI may render a brief "renamed" indicator the
  first time after the change settles.
- Same key, different metadata_json → field-level merge (counts
  monotonically rise; tags accumulate).
- Same key, different kind → reject the newer write. A chat that's
  been seen as a "supergroup" shouldn't decay to "dm".

## API surface

External workspaces are accessed through the runtime ops layer,
which both the Tauri desktop IPC and (eventually) the dashboard HTTP
API consume. Currently the surface is Tauri-IPC only.

| Operation | Tauri command | Runtime op |
|---|---|---|
| List for formation | `list_workspaces` | `springtale_runtime::operations::workspaces::list_workspaces` |
| Scan (manual refresh) | `scan_workspaces` | `…::scan_workspaces` |
| Start onboard stream | `start_onboard_stream` | `…::start_onboard_stream` |
| Preview onboard URL | `preview_onboard_url` | `…::preview_onboard_url` |
| Upsert manually (rare) | `upsert_workspace_manual` | `…::upsert_workspace_manual` |
| Delete by key | `delete_workspace` | `…::delete_workspace` |

### Onboard stream pattern

For first-time chat discovery (a user wants to find chats their bot
is in), the desktop opens a short-lived discovery stream:

```
Frontend:                 Tauri:                  Runtime:
─────────────             ────────                ───────
"Start picker" ───►       start_onboard_stream(   spawn_task(
                          session_id              poll connector
                          ) ──────────────►       endpoints,
                                                  emit ChatDiscovered
                                                  per chat found,
                          ◄─── ChatDiscovered ──  stop after N seconds
                          event (per chat)        or first close)
                          (Tauri event emit) ───►
"Confirm pick" ────►      pick chat
                          delete_workspace if not picked
```

The `ChatDiscovered` event is filtered by `session_id` (minted on
the frontend) so two concurrent pickers don't cross-pollinate.

## UI surface

The desktop's `WorkspaceTargetPicker.tsx` overlay renders the
directory for the active formation. The user filters by kind,
clicks a destination, and the picker resolves the URI back to the
caller (typically a recipe authoring form or a send-message form).

## Authoring recipes with workspaces

A recipe that targets a specific destination can either:

- **Use a URI string in the connector config.** Recipe author
  writes `chat_id = "telegram://chat/12345"`; the connector's
  `extract_id_for_scheme()` translates to the raw id at dispatch.
- **Use the workspace key as a recipe input.** Recipe declares an
  `InputField` of kind `Workspace` (planned — Phase C); the UI
  renders the workspace target picker; the user selects a real
  destination from their directory.

The second is the W2.C / D1 happy path — recipes pick from existing
workspaces rather than asking the user to copy/paste chat IDs.

## Gotchas

- **The harvester only fires on dispatched events.** A chat the bot
  is in but never receives a message from won't appear in the
  directory until the next message arrives. This is intentional —
  enumerable discovery is a leak vector. Use the **onboard stream**
  pattern for an explicit one-shot discovery scan.
- **Workspaces don't survive cross-formation moves.** Removing an
  agent from formation A and adding them to formation B doesn't
  carry workspaces with them. The new formation rebuilds its
  directory from observed events.
- **Display names can change.** A renamed chat shows up with the
  new name on the next message; UI should treat names as advisory.
- **Two connectors, same chat = two workspaces.** A user bridged
  Telegram + Signal sees the same conversation under two
  workspace_keys. Cross-connector dedup is a recipe-author concern,
  not a directory-layer one.
- **The `metadata_json` shape is per-connector.** Telegram puts
  `{ "member_count": N }`; Slack puts `{ "is_archived": false }`.
  Recipe authors shouldn't assume a shape — query specific fields.

## See also

- [`docs/guide/cross-formation.md`](cross-formation.md) — chitchat
  gossip layer that replicates workspaces within a formation.
- [`docs/guide/mental-model.md`](mental-model.md) — the broader
  shared mental model the workspace directory extends.
- [`docs/reference/connectors/`](../reference/connectors/) — per-connector
  trigger/action reference; mention-extraction details per connector.
- `crates/springtale-cooperation/src/mental_model/external_workspaces.rs`
- `crates/springtale-runtime/src/operations/workspaces/`
- `crates/springtale-connector/src/mention.rs`
- `crates/springtale-connector/src/workspace_key.rs`
