# Rekindle Architecture
## Veilid-Native Decentralized Gaming Chat

### Version 3.0 — Flat SMPL Governance · Universal Schema · No Node Above Another

> Based on code audit of the `communities` branch, research into VeilidChat/SimpleX/Briar/Xfire,
> Death Stranding Chiral Network analysis (Q-pid equations as protocol specification),
> and Veilid's founding principle: "All nodes are equal in the eyes of the network."

---

## 1. System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        SolidJS Frontend                         │
│  Windows · Components · Stores · Handlers · Styles              │
│  (holds no business logic — renders state, forwards actions)    │
├──────────────────────────┬──────────────────────────────────────┤
│      Tauri 2 IPC Bridge  │  Commands (FE→Rust) · Events (Rust→FE) │
│      Window mgmt · Tray  │  Plugins · Single-instance            │
├──────────────────────────┴──────────────────────────────────────┤
│                        Pure Rust Crates                         │
│  ┌──────────────────┐ ┌──────────────────┐ ┌─────────────────┐ │
│  │ rekindle-protocol│ │  rekindle-crypto  │ │ rekindle-voice  │ │
│  │ DHT records      │ │  Ed25519 identity │ │ Opus 48kHz      │ │
│  │ Cap'n Proto codec│ │  Signal Protocol  │ │ RNNoise + AEC3  │ │
│  │ Community envelopes│ │ MEK (AES-256-GCM)│ │ Jitter buffer   │ │
│  │ SMPL channel ops │ │  HKDF pseudonyms  │ │ cpal I/O        │ │
│  └──────────────────┘ └──────────────────┘ └─────────────────┘ │
│  ┌──────────────────┐ ┌──────────────────┐                     │
│  │rekindle-game-detect│ │  rekindle-utils  │                     │
│  │ Process scanning  │ │  Timestamps etc  │                     │
│  └──────────────────┘ └──────────────────┘                     │
├─────────────────────────────────────────────────────────────────┤
│                       Veilid Network                            │
│                                                                 │
│  ┌─────────────┐  ┌────────────┐  ┌──────────────────────────┐ │
│  │  DHT Store   │  │ app_message│  │     Private Routes       │ │
│  │ SMPL o_cnt=0│  │ app_call   │  │  Sender + Receiver       │ │
│  │ records      │  │ (transport)│  │  anonymity               │ │
│  └─────────────┘  └────────────┘  └──────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │             VICE (NAT Traversal)                          │   │
│  │  Direct → Hole-punch → Signal-reverse → Relay            │   │
│  │  (transparent to all layers above)                        │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

**Design principle:** All communication routes through Veilid. No external transport. No IP leakage. VICE handles NAT traversal transparently for every connection.

---

## 2. The Three-Path Delivery Model

Messages reach peers through three independent paths. Any single path succeeding is sufficient.

```
                    ┌─────────────────────────┐
                    │      SENDER (Alice)      │
                    └────┬──────┬──────┬───────┘
                         │      │      │
          ┌──────────────┘      │      └──────────────┐
          ▼                     ▼                      ▼
   ╔═════════════╗    ╔══════════════╗    ╔═══════════════════╗
   ║   PATH 1    ║    ║    PATH 2    ║    ║      PATH 3       ║
   ║  SMPL Write ║    ║   Gossip     ║    ║  Watch + Inspect   ║
   ║  (PRIMARY)  ║    ║ (SECONDARY)  ║    ║   (CONSISTENCY)    ║
   ╠═════════════╣    ╠══════════════╣    ╠═══════════════════╣
   ║             ║    ║              ║    ║                    ║
   ║ Write msg   ║    ║ app_message  ║    ║ watch_dht_values   ║
   ║ to own SMPL ║    ║ fan-out to   ║    ║ on channel SMPL    ║
   ║ subkey in   ║    ║ D gossip     ║    ║ records.           ║
   ║ channel     ║    ║ peers via    ║    ║                    ║
   ║ record.     ║    ║ imported     ║    ║ inspect_dht_record ║
   ║             ║    ║ private      ║    ║ every 60s for      ║
   ║ Retry with  ║    ║ routes.      ║    ║ lightweight gap    ║
   ║ backoff on  ║    ║              ║    ║ detection.         ║
   ║ failure.    ║    ║ Fire-and-    ║    ║                    ║
   ║             ║    ║ forget. TTL  ║    ║ Polling fallback   ║
   ║ Persists    ║    ║ decrement +  ║    ║ for failed watches ║
   ║ for offline ║    ║ dedup cache. ║    ║ (Veilid #377).     ║
   ║ catchup.    ║    ║              ║    ║                    ║
   ╚══════╤══════╝    ╚══════╤═══════╝    ╚═════════╤═════════╝
          │                  │                       │
          ▼                  ▼                       ▼
   ┌─────────────────────────────────────────────────────────┐
   │                    RECEIVER (Bob)                        │
   │                                                         │
   │  Merge from all paths → Dedup by message_id             │
   │  → Sort by Lamport timestamp → Decrypt with MEK         │
   │  → Store in SQLite → Emit to frontend                   │
   └─────────────────────────────────────────────────────────┘
```

### Why DHT-First, Not Gossip-First

Early research attempted `app_call` (confirmed delivery) for every gossip message. This caused Veilid connection exhaustion during bootstrap (30+ concurrent operations in the first 5 seconds). The corrected model:

| Concern | Solution |
|---------|----------|
| **Durability** | SMPL channel record (Path 1) — survives sender going offline |
| **Low latency** | Gossip broadcast (Path 2) — sub-second to online peers |
| **Consistency** | Watch + inspect (Path 3) — catches what gossip drops |
| **Confirmed delivery** | `app_call` reserved for critical ops only (MEK delivery, file chunks) |

---

## 3. Single-Tier Operation: Gossip Mesh + SMPL Governance CRDT

```
┌─────────────────────────────────────────────────────────────┐
│                  ALL OPERATIONS: PEER MESH                    │
│              (every member is a full peer)                    │
│                                                              │
│  REAL-TIME (gossip):                                         │
│  Chat messages · Typing · Reactions · Presence · Voice       │
│  Transport: app_message fan-out + SMPL write                 │
│                                                              │
│  GOVERNANCE (SMPL CRDT):                                     │
│  Channel CRUD · Role management · Bans · Invites · MEK      │
│  Transport: SMPL governance record writes (self-sovereign)   │
│  Authority: client-side CRDT merge + permission validation   │
│                                                              │
│  Auth: Ed25519 signature on every envelope + governance entry │
│  Ordering: Lamport timestamps with deterministic tiebreak    │
│  Dedup: FIFO 1024-entry cache (community_id, sender, key)   │
│  Privacy: Private routes (receiver) + Safety routes (sender) │
│  NO COORDINATOR. NO OWNER. NO PRIVILEGED NODES.              │
└─────────────────────────────────────────────────────────────┘
```

**Why no coordinator:** Veilid's founding principle is "no nodes are special." A coordinator is a special node. The v1.0 architecture used a DFLT manifest with a single owner — this created a single point of failure where the creator's departure froze all governance. The corrected architecture uses SMPL records with `o_cnt: 0` for governance. Every member with the appropriate role can write governance entries to their own subkey. Every client merges all governance subkeys independently using deterministic CRDT rules. Permission enforcement is client-side: the reader validates, not the writer.

---

## 4. Community DHT Record Architecture

A community uses **multiple DHT records**, all with the **same universal schema**.

```
         COMMUNITY IDENTITY = REGISTRY RECORD KEY (SMPL, permanent)
         governance_key discovered via bootstrap or peer invite
                    │
    ┌───────────────┼───────────────┬──────────────┬──────────────┐
    ▼               ▼               ▼              ▼              ▼
┌────────┐    ┌──────────┐    ┌────────┐    ┌────────┐    ┌────────┐
│BOOTSTRAP│   │GOVERNANCE│    │REGISTRY│    │CHANNEL │    │CHANNEL │
│ DFLT(1) │   │  SMPL    │    │  SMPL  │    │  SMPL  │    │  SMPL  │
│ optional│   │o_cnt = 0 │    │o_cnt= 0│    │o_cnt= 0│    │o_cnt= 0│
└────────┘    └──────────┘    └────────┘    └────────┘    └────────┘
  immutable    governance     presence &     per-channel    per-channel
  pointers     entries +      route_blob     messages       messages
               CRDT merge     per member     per member     per member
```

### The Universal Schema (The Q-pid Equation)

Every SMPL record in the community uses the same schema:

```
DHTSchema::SMPL {
  o_cnt: 0,                    // Schwarzschild: creation keypair is spent
  members: [                   // Entanglement: derive from shared seed
    Member { m_key: derive(seed, 0).pubkey,   m_cnt: 1 },
    Member { m_key: derive(seed, 1).pubkey,   m_cnt: 1 },
    ...
    Member { m_key: derive(seed, 254).pubkey, m_cnt: 1 },
  ]  // 255 slots (Veilid MAX_WRITER_COUNT=256, owner=1)
}
```

**o_cnt: 0** — The creation keypair grants NOTHING after `create_dht_record`. No owner subkeys. No privileged node. The keypair is generated randomly per record and discarded.

**m_cnt: 1** — Each member gets exactly one subkey. Their presence, their messages, their governance entries. One slot per member per record. Same everywhere.

**255 slots** — Veilid hard limit: MAX_WRITER_COUNT=256, owner counts as 1. Every node derives the same 255 keypairs from the same slot_seed via `derive_slot_keypair(seed, index)`.

### Record 0 (optional): Bootstrap Pointer (DFLT, 1 subkey)

Immutable after creation. Contains pointers only, no governance state:

```
{
  governance_key: "VLD0:...",    // SMPL governance record
  registry_key: "VLD0:...",      // SMPL member registry
  community_name: "...",
  community_description: "..."
}
```

The invite entry point — deep link resolves to this key. Alternatively, peer invites (PATH B) share the keys directly and bypass this entirely.

### Record 1: Governance (SMPL, o_cnt: 0, 255 member subkeys)

Every member has a governance subkey. Every member CAN write. Whether their writes are honored depends on the CRDT permission validation.

```
Member Subkeys (0-254): Each member writes Vec<GovernanceEntry>

GovernanceEntry types:
  ChannelCreated    { name, channel_smpl_key, channel_type, lamport }
  ChannelArchived   { channel_id, lamport }
  RoleDefinition    { role_id, name, permissions_bitmask, position, color, lamport }
  RoleAssignment    { member_pseudonym, role_id, lamport }
  BanEntry          { banned_pseudonym, reason, lamport }
  UnbanEntry        { unbanned_pseudonym, lamport }
  CommunityMeta     { name, description, icon_hash, banner_hash, lamport }
  PinAction         { channel_id, message_id, pinned: bool, lamport }
  MEKGenerationBump { generation, rotator_pseudo, lamport }
  AdminDelete       { target_message_id, channel_id, reason, lamport }
  ChannelUpdated    { channel_id, new_name, new_topic, new_position, lamport }
  ChannelConfig     { channel_id, slowmode_seconds, nsfw: bool, lamport }
  CategoryCreated   { category_id, name, position, lamport }
  CategoryUpdated   { category_id, name, position, lamport }
  ChannelCategoryAssignment { channel_id, category_id, position_in_category, lamport }
  StageConfig       { channel_id, stage_mode: bool, speakers: Vec<pseudonym>, lamport }
  EventCreated      { event_id, title, description, start_time, end_time,
                      location_channel_id, cover_image_hash, lamport }
  SegmentAdded      { segment_index, registry_key, governance_key, slot_range }
  ChannelSegmentLinked { channel_name, segment_1_key, segment_2_key }
  ThreadCreated     { parent_channel_id, title, thread_record_key, lamport }
  OnboardingConfig  { questions, welcome_message, guide_steps, lamport }
  WelcomeScreen     { channel_descriptions: Vec<(channel_id, desc)>, lamport }
  GovernanceOverflow { overflow_record_key }
  GovernanceMigration { new_governance_key, reason, lamport }

Validation: reader checks writer's signature → looks up writer's role
in merged CRDT state → validates permission for this entry type →
accept or ignore.

Genesis: entries at Veilid sequence number 1 are valid without
permission checking (Schwarzschild — the initial conditions).
All subsequent entries validated against merged state.
```

### Record 2: Member Registry (SMPL, o_cnt: 0, 255 member subkeys)

```
Member Subkeys (0-254): Each member writes their own

  MemberPresence {
  MemberPresence {
    pseudonym_key, display_name: Option<String>,
    status, custom_status, custom_status_emoji,
    current_voice_channel, route_blob, last_heartbeat,
    game_info, avatar_ref, banner_ref, bio, pronouns,
    theme_color, badges,
    in_call: bool, call_type: Option<"direct" | "group" | "community">,
    push_relay_route: Option<Vec<u8>>,
    event_rsvps: Vec<EventRSVP { event_id, status: going|interested|declined }>,
    history_ranges: Vec<HistoryRange { channel_id, oldest_lamport, newest_lamport }>
  }
  }

  Also carries GovernanceMigration entries (rogue admin escape valve —
  self-sovereign subkeys that no rogue can overwrite).

Member list reconstruction: inspect_dht_record(registry_key, all_subkeys)
→ sequence > 0 means occupied → read occupied subkeys.
No MemberIndex owner subkey. inspect is the primary method.
```

### Record 3+: Channel Message Records (SMPL, o_cnt: 0, one per channel)

```
Member Subkeys (0-254): Each member writes their own entries

  Vec<ChannelEntry> (MEK-encrypted)

  enum ChannelEntry {
    Message {
      message_id, author_pseudonym, content, mek_generation,
      timestamp, lamport_ts, sequence, reply_to,
      attachments, embeds, flags,
      poll: Option<Poll { question, options: Vec<String>,
                          multi_vote: bool, expires_at: Option<u64> }>
    },
    Reaction {
      target_message_id, emoji, added: bool, lamport
    },
    Edit {
      target_message_id, new_ciphertext, lamport
    },
    Delete {
      target_message_id, lamport   // tombstone — clients stop displaying
    },
    Forward {
      original_message_id, original_channel_id,
      original_author_pseudonym, content_snapshot,  // re-encrypted to destination MEK
      forwarded_at, lamport
    },
    PollVote {
      target_message_id, option_indices: Vec<u32>, lamport
    },
    AttachmentCached {
      hash, available_since, lamport  // signals this peer has the file
    },
    HandRaise {
      channel_id, lamport  // request to speak in stage channel
    },
  }

  overflow_next: Option<String>  // in subkey header, links to personal overflow record

  Reactions: Bob reacts to Alice's message → Bob writes Reaction to BOB's
  subkey. Clients read all subkeys, collect all Reactions per message_id,
  merge counts. Same SSS pattern — independent contributions, merged locally.

  Edits: Alice edits her message → Alice writes Edit to her own subkey.
  Clients display the latest Edit for each message_id from the same author.

  Deletes: Alice deletes her message → Alice writes Delete to her own subkey.
  Clients stop displaying. Honest UX: "Message deleted" placeholder.
  Deletion is a request honored by well-behaved clients, not a guarantee —
  peers who already received it have it in SQLite. Same as SSB.

  Forwards: Alice forwards Bob's message to another channel. Alice writes
  Forward with content_snapshot (re-encrypted to destination channel's MEK)
  to her own subkey in the destination channel. Self-contained — readers
  don't need access to the original channel. Like lost cargo delivered
  to a different destination.

  Polls: Poll in a Message. Votes via PollVote entries from each voter's
  own subkey. Clients merge all PollVote per message_id. Last-writer-wins
  per (voter, message_id). Same SSS merge as Reactions.

  AttachmentCached: signals this peer has cached a file. Clients scan for
  these when requesting files — try peers with matching hash via app_call.

  HandRaise: request to speak in a stage channel. Admins see it in UI
  and can add the member to StageConfig.speakers via governance.

  Page trimming: when subkey exceeds ~30KB, oldest entries dropped from DHT
  (still in local SQLite). Overflow: personal SMPL record linked via header.

  Ordering: All members read all subkeys → collect Message entries →
  merge-sort by (lamport_ts, sender_pseudonym) → deterministic view.
  Reactions/Edits/Deletes applied as overlays on the sorted message list.
  AdminDelete from governance entries also applied as overlay —
  clients check governance for AdminDelete targeting each message_id.
  Same client-side enforcement as bans.

  No owner subkeys for metadata/pins/archive — all moved to
  governance entries (PinAction) or client-computed (message counts).
```

### CRDT Merge Rules

All governance entries across all subkeys merged deterministically. Every client reading the same entries MUST arrive at the same state:

- **Channel list:** UNION of all ChannelCreated minus ChannelArchived. Both exist = not archived.
- **Role definitions:** Last-writer-wins per role_id (highest Lamport, tiebreak: lowest pseudonym key).
- **Role assignments:** Last-writer-wins per member_pseudonym (highest Lamport).
- **Ban list:** UNION of all BanEntry. UnbanEntry counters a BanEntry (last-writer-wins on banned/unbanned per pseudonym).
- **Community metadata:** Last-writer-wins on single entry (highest Lamport).
- **MEK generation:** Highest generation number wins. Tiebreak on rotator pseudonym.
- **Pins:** Last-writer-wins per (channel_id, message_id) on pinned flag.
- **Admin deletes:** UNION of all AdminDelete entries. Validated: writer must have MANAGE_MESSAGES. Clients stop displaying targeted messages. Same enforcement model as bans — client-side, reader-validated.
- **Threads:** UNION of all ThreadCreated per parent_channel_id. Lazy SMPL record creation. Auto-archive: clients stop watching threads inactive > 7 days.
- **Categories:** UNION of all CategoryCreated. CategoryUpdated is LWW per category_id. ChannelCategoryAssignment is LWW per channel_id. Display-only grouping.
- **Channel config:** LWW per channel_id for slowmode_seconds, nsfw flag. ChannelUpdated is LWW per field per channel_id (name, topic, position).
- **Stage config:** LWW per channel_id. Latest StageConfig determines speakers list and stage mode.
- **Events:** UNION of all EventCreated. RSVPs live in MemberPresence (scanned via presence reads, not governance).
- **Onboarding/Welcome:** Last-writer-wins (highest Lamport). Only one active config at a time.

### Channel Entry Merge Rules

ChannelEntry variants across all member subkeys per channel merged locally:

- **Messages:** Collect all Message entries → merge-sort by (lamport_ts, sender_pseudonym).
- **Reactions:** Collect all Reaction entries per target_message_id. Group by emoji. Count distinct voters. Toggle: latest Reaction per (voter, message_id, emoji) wins.
- **Edits:** Latest Edit per (target_message_id, author=original_author) wins. Only the original author's edits are honored.
- **Deletes:** Tombstone per target_message_id from the original author. AdminDelete from governance also applied.
- **Forwards:** Displayed inline in message list at the Forward's lamport position.
- **PollVotes:** Collect per target_message_id. LWW per (voter_pseudonym, message_id). Multi-vote honored if poll.multi_vote is true. Votes after poll.expires_at ignored.
- **AttachmentCached:** Collected per hash. Used to locate peers when requesting files.
- **HandRaise:** Displayed to admins in stage channel UI. Transient — cleared when speaker list updates.

---

## 5. Message Send Lifecycle

```
  send_channel_message()
  │
  ├─ 1. PERMISSION CHECK
  │     require_permission(SEND_MESSAGES)
  │
  ├─ 2. ENCRYPT
  │     MEK from mek_cache → AES-256-GCM encrypt body
  │
  ├─ 3. PERSIST LOCALLY (before any network op)
  │     insert_channel_message() → SQLite
  │     (message is safe even if all network ops fail)
  │
  ├─ 4. INCREMENT LAMPORT
  │     gossip.lamport_counter += 1
  │
  ├─ 5. BUILD ENVELOPE
  │     CommunityEnvelope::ChatMessage {
  │       channel_id, message_id, author_pseudonym,
  │       ciphertext, mek_generation, timestamp,
  │       lamport_ts, sequence
  │     }
  │
  ├─ 6. SIGN
  │     SignedEnvelope: pseudonym signing key + Ed25519 signature
  │     Insert into dedup cache (prevent processing own forward)
  │
  ├─ 7. PATH 1: SMPL WRITE (background, retry on failure)
  │     write_member_message(channel_key, my_subkey, my_keypair, msg)
  │     → Unflushed queue with exponential backoff
  │
  ├─ 8. PATH 2: GOSSIP BROADCAST
  │     For each D-peer in gossip.peers:
  │       import_remote_private_route(peer.route_blob)
  │       app_message(Target::RouteId(route_id), signed_bytes)
  │     Fire-and-forget. If route import fails → log and skip.
  │
  └─ 9. LOCAL ECHO
        Emit ChatEvent::MessageReceived to frontend
```

## 6. Message Receive Lifecycle

```
  VeilidUpdate::AppMessage received
  │
  ├─ 1. ROUTE: Voice packet (prefix 'V')? → voice engine
  │            Community SignedEnvelope (JSON)? → community handler
  │            Other? → standard message handler
  │
  ├─ 2. DESERIALIZE SignedEnvelope
  │
  ├─ 3. VERIFY Ed25519 signature against sender_pseudonym
  │
  ├─ 4. CHECK TTL > 0 (drop expired gossip)
  │
  ├─ 5. DEDUP CHECK
  │     dedup_cache.check_and_insert(community_id, sender, dedup_key)
  │     Already seen? → drop silently
  │
  ├─ 6. UPDATE LAMPORT (Gap K fix)
  │     gossip.lamport_counter = max(local, received_lamport_ts) + 1
  │
  ├─ 7. SEQUENCE GAP CHECK
  │     Compare received_seq against peer_sequences[(sender, channel)]
  │     Gap detected? → queue SyncRequest for missing range
  │     Update peer_sequences
  │
  ├─ 8. GOSSIP FORWARD (if TTL > 1)
  │     Decrement TTL, forward to own D-peers (excluding sender)
  │     via app_message (fire-and-forget)
  │
  ├─ 9. PROCESS
  │     Decrypt ciphertext with channel MEK
  │     Store in SQLite
  │     Emit ChatEvent::MessageReceived to frontend
  │
  └─ 10. Also received via PATH 3 (DHT watch)?
         Dedup by message_id prevents double-processing
```

---

## 7. Voice Architecture

```
                  ≤4 members: FULL-MESH P2P
  ┌────────┐                                  ┌────────┐
  │ Alice  │◄────── Opus frames (MEK) ───────►│  Bob   │
  │        │◄────── via app_message    ───────►│        │
  └────┬───┘       SafetySelection::Unsafe     └───┬────┘
       │           (VICE handles NAT)              │
       │                                           │
       └──────────────►┌────────┐◄─────────────────┘
                       │ Carol  │
                       └────────┘


                  >4 members: MUTUAL-AID SFU RELAY
  ┌────────┐         ┌──────────────┐         ┌────────┐
  │ Alice  │────────►│              │────────►│  Bob   │
  └────────┘  Opus   │ RELAY PEER   │  Opus   └────────┘
  ┌────────┐ frames  │ (lowest XOR  │ frames  ┌────────┐
  │ Carol  │────────►│  to channel) │────────►│  Dave  │
  └────────┘  (MEK   │ Opaque fwd   │  (MEK   └────────┘
              encr.) │ No decrypt   │  encr.)
                     └──────────────┘
```

### Audio Pipeline

```
  Capture (cpal) → RNNoise denoise → AEC3 echo cancel
  → VAD (voice activity detect) → Opus encode (48kHz mono, 32kbps)
  → MEK encrypt → app_message to peers/relay

  Receive → MEK decrypt → Opus decode → Jitter buffer
  → Audio mixer (multi-participant) → Playback (cpal)
```

**Transport:** Opus frames at ~200 bytes each. Trivial for `app_message`. Voice uses `SafetySelection::Unsafe` routing context (no safety hops) because frames are already encrypted and sender identity is hidden by pseudonyms. Saves ~50-100ms latency per hop.

**VICE:** NAT traversal is transparent. Direct when possible, hole-punched behind NAT, relayed as last resort. No application-level ICE/STUN/TURN.

**Shared pipeline:** The same audio pipeline (cpal → RNNoise → AEC3 → Opus → encrypt → app_message) serves both community voice channels AND direct/group calls. The differences are signaling (community: presence subkey; calls: app_call offer) and key management (community: channel MEK; calls: ECDH or initiator-generated key). See §7.5.

### 7.5 Direct and Group Voice Calls (Chiralgrams)

Community voice channels are persistent — you join/leave a channel. Direct and group calls are ephemeral — they exist only for the duration of the call, with no DHT record.

**Chiral Network parallel:** Chiralgrams. Direct holographic communication between endpoints. No terminal infrastructure needed — just two active routes and the Q-pid equations.

#### Direct Call (1-on-1)

```
  Alice                                          Bob
    │                                              │
    ├─ Read Bob's friend profile → route_blob      │
    │                                              │
    ├─ ECDH(alice_key, bob_pubkey)                 │
    │  → shared_secret → HKDF → call_key           │
    │                                              │
    ├─ CallOffer { session_id, codec } ─app_call──►│
    │                                              ├─ Ring UI (30s timeout)
    │                                              │
    │◄──app_call── CallAccept { session_id } ──────┤
    │                                              │
    ├─ Opus → call_key encrypt ────app_message────►│
    │◄────app_message──── call_key encrypt ← Opus──┤
    │     (SafetySelection::Unsafe, both ways)      │
    │                                              │
    ├─ CallEnd { session_id } ───app_message──────►│
    │  (or route dies → call drops naturally)       │
```

**Key exchange:** X25519 ECDH between friend profile keys → HKDF → symmetric call_key. XChaCha20-Poly1305 encryption on Opus frames (same primitives Veilid uses internally). Key exists only for the call duration. No DHT record. No SMPL record. Pure real-time.

**Signaling:** CallOffer via app_call (confirmed delivery — Bob's client must respond). CallAccept/CallDecline/CallCancel via app_call. CallEnd via app_message (fire-and-forget — if it fails, route death handles cleanup).

**Ringing:** 30-second ring timer. Alice can cancel (CallCancel). Bob can decline (CallDecline). No response → missed call. Missed calls stored in local SQLite, shown in call history.

**Push wake:** If Bob is backgrounded, the push relay detects activity and fires a push notification. Bob's app wakes, reconnects to Veilid, receives CallOffer, shows incoming call UI.

#### Group Call (Outside Community)

```
  Initiator (Alice)               Participants (Bob, Carol, Dave)
    │                                              │
    ├─ Generate random call_key (AES-256)          │
    │                                              │
    ├─ For each participant:                       │
    │    Wrap call_key to their pubkey             │
    │    CallOffer { session_id, wrapped_key }     │
    │    ──app_call──►                             │
    │                                              │
    │                    ◄── CallAccept ───────────┤ (each)
    │                                              │
    │  ≤4: Full mesh P2P                           │
    │  Each ←──app_message──► Each                 │
    │                                              │
    │  >4: SFU relay                               │
    │  relay = lowest hash(session_id + pseudonym)  │
    │  Each ──app_message──► Relay ──► Each        │
```

**Key distribution:** Initiator generates random symmetric call_key, wraps to each participant's pseudonym pubkey (X25519), delivers in CallOffer via app_call. Same pattern as MEK delivery but ephemeral.

**Two-mode transport:** Same as community voice. ≤4 full mesh, >4 mutual-aid SFU relay selected by deterministic hash. Relay switches automatically if the relay peer disconnects.

**Adding/removing participants mid-call:** Initiator sends CallOffer to new participant with current call_key. For removal, generate new call_key, distribute to remaining participants via app_call. Departing member's frames are ignored (no new key).

#### Video and Screen Sharing in Calls

Same signaling. Same key exchange. Video frames sent alongside audio via fragmented app_message (§23 interim protocol). Codec negotiation in CallOffer:

```
CallOffer {
  session_id,
  audio_codec: "opus",
  video_codec: Option<"vp9">,
  video_quality: Option<"480p">,
  wrapped_key: Option<Vec<u8>>,  // for group calls
}
```

Screen sharing: toggle on/off via `ScreenShareStart/Stop` control messages in the call's app_message stream. Screen capture frames encrypted with call_key, sent to all participants (or SFU relay).

#### Call Status in Presence

`MemberPresence` extended with:

```
in_call: bool,
call_type: Option<"direct" | "group" | "community">,
```

Clients check this before initiating a call. If Bob is already in a call, Alice's client shows "Bob is on another call" instead of ringing.

---

## 8. Identity and Privacy Model

```
  ┌─────────────────────────────────────────────────────────┐
  │                    MASTER IDENTITY                       │
  │              Ed25519 keypair (Stronghold vault)           │
  │              = Your Rekindle identity                     │
  │              Never exposed to communities                 │
  └──────────┬──────────────┬──────────────┬─────────────────┘
             │              │              │
      HKDF derive    HKDF derive    HKDF derive
             │              │              │
             ▼              ▼              ▼
  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
  │ Community A   │ │ Community B   │ │ Community C   │
  │ Pseudonym     │ │ Pseudonym     │ │ Pseudonym     │
  │ (unlinkable)  │ │ (unlinkable)  │ │ (unlinkable)  │
  └──────────────┘ └──────────────┘ └──────────────┘
```

**Unlinkability:** Your pseudonym in Community A cannot be correlated with your pseudonym in Community B by any observer, including DHT node operators. Derived via HKDF with community_id as context.

### Four-Layer Encryption Stack

```
  Layer 1: Signal Protocol (1:1 DMs)
           X3DH key agreement + Double Ratchet
           Forward secrecy + break-in recovery

  Layer 2: MEK (Community channels)
           Per-channel AES-256-GCM key
           Rotated on member leave/ban
           Distributed via encrypted vault in member registry

  Layer 3: Veilid Transport
           XChaCha20-Poly1305 on every Veilid connection
           Transparent, always-on

  Layer 4: Stronghold Vault (At rest)
           AES-256-GCM + Argon2id KDF
           Protects master keypair, MEKs, Signal sessions
```

### Privacy Routing

```
  ┌──────┐    Safety Route    ┌──────┐    Private Route    ┌──────┐
  │Sender│───► Hop Node A ───►│ Meet │◄─── Hop Node B ◄───│Recvr │
  │      │    (sender priv.)  │Point │    (recvr priv.)    │      │
  └──────┘                    └──────┘                     └──────┘
           Sender IP hidden              Receiver IP hidden
           from receiver                 from sender

  Compiled Route = Safety + Private = 3 hops total (default)
  Configurable: 1-hop for casual chat, 2-hop for high privacy
```

---

## 9. Route Lifecycle

```
  NODE STARTUP
  │
  ├─ api_startup() → VeilidAPI
  ├─ api.routing_context() → rc_safe (default safety, chat/DHT)
  ├─ api.routing_context().with_safety(Unsafe) → rc_voice (no safety, voice)
  ├─ new_private_route() → (route_id, route_blob)
  └─ Publish route_blob to DHT profile + all community registries

  EVERY 120 SECONDS (route_refresh_loop)
  │
  ├─ Reallocate private route (new route_id + blob)
  ├─ Re-publish route_blob to all community SMPL registries
  ├─ Reset needs_initial_sync for all communities
  └─ Trigger presence re-announcement

  ON RouteChange EVENT (dead routes)
  │
  ├─ OUR route died?
  │   └─ forget_private_route() (don't release — already dead)
  │      allocate_fresh_private_route()
  │
  └─ PEER route died?
      └─ Invalidate cached peer routes
         Clear stale route_blobs from peer list
```

---

## 10. Record Lifecycle (VeilidChat Pattern)

```
  ON JOIN / CREATE
  │
  ├─ Open governance record (read all subkeys, merge CRDT)
  ├─ Open member registry (with slot keypair for presence writes)
  ├─ Open ALL channel SMPL records (from governance channel list)
  ├─ Watch governance (all subkeys)
  ├─ Watch channel SMPL records (all member subkeys) ← Gap I fix
  ├─ Track all keys in CommunityRecords struct
  └─ Records stay open for entire session

  DURING SESSION
  │
  ├─ NEVER open/close per operation
  ├─ NEVER re-open writable record as read-only (clobbers writer)
  ├─ Use get_dht_value directly on already-open records
  └─ Keepalive via get_value (touch without re-open)

  ON LEAVE / LOGOUT
  │
  ├─ Stop all background tasks (presence poll, keepalive)
  ├─ Wait for tasks to complete
  ├─ Close all community records tracked in CommunityRecords
  └─ Remove from global tracking
```

---

## 11. Sync Protocol

### Real-Time: Gossip + Watch

```
  SENDER writes message
  │
  ├─ PATH 2: Gossip → online peers get it in ~100ms
  │
  └─ PATH 1: SMPL write → triggers ValueChange on watchers
                           │
                           ▼
                     RECEIVER's watch callback fires
                     → read changed subkey
                     → merge new messages
                     → dedup by message_id
```

### Periodic: inspect_dht_record (Gap J Fix)

```
  EVERY 60 SECONDS PER COMMUNITY
  │
  ├─ For each channel SMPL record:
  │   └─ inspect_dht_record(key, all_subkeys, SyncGet)
  │      → Returns local_seq vs network_seq per subkey
  │
  ├─ For any subkey where network_seq > local_seq:
  │   └─ get_dht_value(key, subkey, force_refresh: true)
  │      → Merge new messages into SQLite
  │
  └─ Cost: one metadata call per channel per minute
           Data transfer only for changed subkeys
```

### Gap Recovery: Sequence Detection

```
  ON RECEIVE (message with sequence N+3, last seen was N)
  │
  ├─ Gap detected: missing sequences N+1, N+2
  │
  ├─ Option A: Read sender's SMPL subkey directly
  │   └─ get_dht_value(channel_key, sender_subkey, force_refresh)
  │      Messages N+1, N+2 should be there (SMPL write is primary)
  │
  └─ Option B: SyncRequest via app_call to any online peer
      └─ Peer reads the SMPL subkey and returns missing messages
```

---

## 12. Community Join Flow (Self-Sovereign, No Coordinator)

```
  NEW MEMBER                           COMMUNITY
  │                                    │
  1. Receive invite (deep link / paste / peer share)
  │                                    │
  2. Decrypt InviteSecrets             │
  │  (slot_seed, registry_key,         │
  │   governance_key, MEK)             │
  │                                    │
  3. Open governance (read-only)       │
  │  Merge CRDT → get channels,        │
  │  roles, bans, community name       │
  │                                    │
  4. Derive pseudonym                  │
  │  HKDF(master_key, community_id)    │
  │                                    │
  5. Check ban list (client-side)      │
  │  If banned → abort                 │
  │                                    │
  6. Scan registry for empty slot      │
  │  inspect_dht_record → find seq=0   │
  │                                    │
  7. Derive slot keypair               │
  │  derive_slot_keypair(seed, index)  │
  │                                    │
  8. Claim slot (self-sovereign)       │
  │  Write MemberPresence → re-read    │
  │  → verify own pseudonym → retry    │
  │  on collision (already implemented │
  │  in join.rs)                       │
  │                                    │
  9. Request MEK if stale              │
  │  Broadcast RequestMEK via gossip   │
  │  → any online peer delivers via    │
  │  app_call (deterministic responder │
  │  selection: hash(requester+self)   │
  │  → lowest hash responds)           │
  │                                    │
  10. Open channel SMPL records        │
  │   (from governance channel list)   │
  │   Staggered 200ms between each     │
  │                                    │
  11. Bootstrap gossip peers           │
  │   Read occupied registry subkeys   │
  │   → populate route_blobs           │
  │                                    │
  12. Watch governance + channels      │
  │                                    │
  13. Start presence poll (15s delay)  │
  │                                    │
  14. SMPL catchup                     │
      read_all_channel_messages        │
      for each channel → merge SQLite  │
```

**Invite paths:** PATH B (peer invite) is the primary path. Any member constructs InviteSecrets from their local state (slot_seed, current MEK, registry_key, governance_key) and shares via deep link. The optional DFLT bootstrap record provides a discoverability entry point but is not required.

**Fast path (mutual aid):** Instead of steps 3-6 as independent DHT reads, the new member can request a BootstrapBundle from the inviting member (§20.4). One app_call round trip delivers governance snapshot, registry snapshot, channel keys, MEK, and peer routes. The joiner still verifies independently but starts from a warm cache instead of cold DHT reads.

---

## 13. MEK (Media Encryption Key) Lifecycle — Peer-to-Peer, No Vault

```
  CREATE COMMUNITY
  │
  ├─ Generate MEK: AES-256-GCM key, generation=1
  ├─ Cache in mek_cache + persist to local Stronghold
  └─ Write MEKGenerationBump { generation: 1 } to governance subkey
     (NO MEKVault. NO owner subkey. MEK is peer-to-peer.)

  MEMBER JOINS
  │
  ├─ InviteSecrets contains current MEK (may be stale)
  ├─ If MEK generation mismatch detected on first decrypt:
  │    Broadcast RequestMEK via gossip mesh (TTL+dedup+forward —
  │    peers who don't have the MEK forward the request onward,
  │    propagating through the mesh until it reaches a holder)
  │    Any online peer delivers current MEK via app_call
  │    Deterministic responder: hash(requester+self) → lowest responds
  └─ Cache in mek_cache + persist to Stronghold

  MEMBER LEAVES / BANNED
  │
  ├─ Deterministic rotator selection:
  │    hash(departed_pseudonym + own_pseudonym) → lowest hash rotates
  │    If rotator doesn't act within 30s, second-lowest takes over
  ├─ Rotator: generate new MEK, increment generation
  ├─ Wrap per remaining member (X25519 to each pseudonym pubkey)
  ├─ Deliver via app_call to each online member
  ├─ Broadcast MEKRotated { generation } via gossip
  ├─ Write MEKGenerationBump to own governance subkey
  └─ New messages use new generation
     (old MEK kept in cache for decrypting historical messages)

  OFFLINE MEMBER RETURNS
  │
  ├─ Reads governance → finds MEKGenerationBump > local generation
  ├─ Broadcasts RequestMEK via gossip
  └─ Any online peer delivers current MEK via app_call
```

**Why no vault:** MEKVault was an owner subkey in the registry. With o_cnt: 0, there are no owner subkeys. MEK for a 100-member community would be ~8KB per rotation entry — three rotations would fill a governance subkey. Instead, MEK is transient: delivered peer-to-peer, cached locally in Stronghold. The governance record stores only the generation counter so clients know which generation is current.

---

## 14. Transport Matrix

| Data Type | Transport | Privacy | Routing Context |
|-----------|-----------|---------|-----------------|
| Text messages | SMPL write + gossip app_message | Full | rc_safe |
| Governance entries | SMPL governance write | Full | rc_safe |
| Typing indicators | gossip app_message | Full | rc_safe |
| Presence / DHT | DHT subkey writes | Full | rc_safe |
| Community voice | app_message (MEK encrypted) | Full | **rc_voice** (Unsafe) |
| Direct call signaling | app_call (CallOffer/Accept/Decline) | Full | rc_safe |
| Direct call audio | app_message (ECDH call_key encrypted) | Full | **rc_voice** (Unsafe) |
| Group call audio | app_message (initiator call_key encrypted) | Full | **rc_voice** (Unsafe) |
| Video (interim) | Fragmented app_message (call_key or MEK) | Full | **rc_voice** (Unsafe) |
| Screen sharing | Fragmented app_message (call_key or MEK) | Full | **rc_voice** (Unsafe) |
| File attachments | Chunked app_call peer-to-peer | Full | rc_safe |
| MEK distribution | app_call (peer-to-peer confirmed) | Full | rc_safe |
| RequestMEK | gossip app_message | Full | rc_safe |
| Strand relay forward | app_message (RelayEnvelope, encrypted to recipient) | Full | rc_safe |
| Presence cache query | app_message via relay route | Full | rc_safe |
| Watch relay notification | app_message (subkeys_changed only, no content) | Full | rc_safe |
| Bootstrap bundle | app_call (single response snapshot) | Full | rc_safe |
| RequestMEK (gossip-forwarded) | gossip app_message with TTL+dedup+forward | Full | rc_safe |
| DM messages | SMPL write (2-member record) + watch | Full | rc_safe |

---

## 15. Remaining Work (Priority Order)

### Critical (Reliability)

| # | Gap | Fix | Effort |
|---|-----|-----|--------|
| A | SMPL write fire-and-forget | Unflushed queue with retry + backoff | M |
| I | No channel record watches | watch_dht_values on join per channel | S |
| K | Lamport not updated on receive | max(local, received) + 1 in handler | S |
| H | Safety mode not applied | Dual routing context (safe + voice) | S |
| G | Record lifecycle bugs | CommunityRecords tracker, open-once | M |

### Critical (Flat Governance Migration)

| # | Gap | Fix | Effort |
|---|-----|-----|--------|
| — | DFLT manifest → SMPL governance | Create governance record with o_cnt=0, migrate all manifest subkey data to governance entries, update create/join flows | L |
| — | Owner subkeys → member subkeys | Registry o_cnt: 0 (remove MemberIndex + MEKVault owner subkeys), channel o_cnt: 0 (remove metadata/pins owner subkeys) | L |
| — | CRDT merge engine | Governance entry types, merge rules, client-side permission validation chain | L |
| — | Peer MEK delivery | Gossip-level RequestMEK handler (any member responds), deterministic rotator selection | M |
| — | Peer invites (PATH B) | Member constructs InviteSecrets from local state, modified join to accept inline secrets | M |

### Important (Consistency)

| # | Gap | Fix | Effort |
|---|-----|-----|--------|
| B | No sync protocol | peer_sequences + inspect_dht_record loop | L |
| J | No inspect_dht_record | 60s sync loop per community | M |
| C | Gossip no ACK | SMPL = reliability; gossip stays fire-and-forget | S |

### Features (Completeness)

| # | Gap | Fix | Effort |
|---|-----|-----|--------|
| D | Voice >4 not connected | Two-mode transport (mesh / SFU via mutual aid) | L |
| F | No block store | Chunked app_call file transfer (lost cargo pattern) | L |
| — | Plate gate scaling | SegmentAdded governance entry + lazy channel segment creation | L |
| — | Governance fork | GovernanceMigration in registry subkeys for rogue admin recovery | M |
| — | Threads/forums | ThreadCreated governance entry + lazy SMPL record per thread | M |
| — | Gaming identity | Rich GameInfo presence, game server browser SMPL record, join-game protocol | L |
| — | Push notifications | Three-tier escalation: foreground → background fetch → opt-in relay | L |
| — | Strand relay network | Dedicated relay routes, RelayRoster record, forwarding, presence caching | L |
| — | Record warming | Expand keepalive to cycle all community records during idle | S |
| — | History advertisements | history_ranges in MemberPresence, catchup from deepest peer | M |
| — | Watch relay | WatchRelay gossip notification to watchless peers on ValueChange | M |
| — | Bootstrap bundles | BootstrapBundle single-response for new member join | M |
| — | Gossip topology optimization | Per-peer delivery metrics in SQLite, weighted fan-out | M |
| — | File sharing | Chunked transfer with AttachmentCached tracking + local pinning | M |
| — | Search | FTS5 local index with filters (from, in, has, before, after, mentions) | M |
| — | Community discovery | Self-service SMPL directory with public slot_seed, plate gate scaling | M |
| — | DMs | 2-member SMPL record, app_call invite, ECDH key, watch for real-time | M |
| — | Direct voice calls | CallOffer/Accept signaling, ECDH call_key, ring/decline/miss handling | M |
| — | Group voice calls | Initiator-generated call_key, wrapped per-participant, mesh/SFU two-mode | M |
| — | Video in calls | VP9 fragmented app_message alongside audio, codec negotiation in CallOffer | L |
| — | Emoji/stickers/sounds | Governance metadata + eager peer-cache on join (~48MB budget) | M |
| — | Bot SDK | API wrapping rekindle-protocol for headless member nodes | M |
| — | Cross-device sync | Personal DFLT record with ReadState, watch for sync | S |
| — | User blocking | Local SQLite per-pseudonym, client-side filtering | S |
| — | Unread tracking | Local SQLite per-channel high-water mark | S |
| — | Onboarding/welcome | OnboardingConfig + WelcomeScreen governance entry types | S |
| — | Mentions | Client-side parse of @pseudonym/@role/@everyone with permission gating | S |
| — | Categories | CategoryCreated + ChannelCategoryAssignment governance entries | S |
| — | Slowmode | ChannelConfig governance + client-side send enforcement | S |
| — | Forwarding | ChannelEntry::Forward with content snapshot, re-encrypt to destination MEK | S |
| — | Polls | Poll in Message + PollVote ChannelEntry, SSS merge | S |
| — | Events/RSVP | EventCreated governance + event_rsvps in MemberPresence | M |
| — | Stage channels | StageConfig governance + client-side send/receive gating, HandRaise | M |
| — | Display names | display_name in MemberPresence (already in schema) | S |
| — | Audit log view | UI reading governance CRDT chronologically (data already exists) | S |

S = Small (< 1 day), M = Medium (1-3 days), L = Large (3+ days)

---

## 16. Scaling Past 255 Members (The Plate Gate)

Veilid hard limit: MAX_WRITER_COUNT = 256 per SMPL record (owner + 255 members). When the registry fills, a new segment opens — same Q-pid equation applied to a new slot range.

**Trigger:** Any member detects all 255 registry subkeys have sequence > 0 via `inspect_dht_record`.

**Phase 1 — Open the gate (any admin):** Create registry-2 and governance-2 with the universal schema using slots 255..509. Write `SegmentAdded` to own governance subkey in governance-1. All clients discover the new segment during governance merge.

**Phase 2 — Activate terminals (lazy):** Channel segment-2 records are NOT created in batch. When a segment-2 member writes to a channel that has no segment-2 record, they create it. Write `ChannelSegmentLinked` to governance. Other members detect it, open the record, set up watches.

**Cross-segment delivery:** Gossip operates on the mesh, not on records — it crosses segment boundaries naturally. Members in segment 1 receive segment-2 members' messages via gossip immediately. Watch/inspect on segment-2 channel records provides the consistency backstop.

**Fractal scaling:** The pattern repeats for segments 3, 4, and beyond. A 1000-member community has 4 registry segments, 4 governance segments, and channel segments created on demand per active channel. Same schema. Same derivation. Same equation at every level.

---

## 17. Threads and Forums

Each thread is a SMPL record with the universal schema. Same Q-pid equation applied to a conversation branch.

**Creation:** `ThreadCreated { parent_channel_id, title, thread_record_key, lamport }` governance entry from any member with CREATE_THREADS permission. The SMPL record is created lazily — only when the first reply is written, not when the thread is announced.

**Discovery:** Clients read all ThreadCreated entries from governance merge, filtered by parent_channel_id. Forum channels are channels where the primary view is the thread list rather than a linear message feed.

**Auto-archive:** Client-side rule. Stop watching thread records where the latest message lamport is older than a configurable threshold (default 7 days inactive). The DHT record persists via timefall model — decays from caching naturally if nobody reads it. If someone revives the thread, they re-watch and catchup from any peer's SQLite.

**Lifecycle:** Thread records follow the same overflow and segment patterns as channel records. A thread that grows large uses member-owned overflow records. A thread in a >255-member community uses segment-paired records.

---

## 18. Gaming Identity (Rekindle's Differentiator)

This is why someone switches from Discord. The architecture must serve gaming-specific interactions, not just generic chat.

### Rich Presence

Extend GameInfo in MemberPresence:

```
GameInfo {
  game_id: String,           // internal identifier
  game_name: String,         // display name
  map_or_level: Option<String>,
  character_name: Option<String>,
  party_size: Option<u32>,
  max_party: Option<u32>,
  joinable: bool,
  join_data: Option<String>, // game-specific: IP:port, lobby code, Steam join link
  started_at: u64,
  screenshot_hash: Option<String>,  // latest screenshot attachment hash
}
```

**Join game:** When a friend's presence shows `joinable: true`, the client shows a "Join Game" button. Clicking reads `join_data` and launches the game via protocol handler or command line (game-specific logic in `rekindle-game-detect` crate). This is the Xfire feature that made it legendary.

### Game Server Browser

A dedicated SMPL record with the universal schema. Any member can list a server:

```
GameServer {
  game_id, server_name, address, player_count, max_players,
  map, game_mode, password_protected: bool, lamport,
  heartbeat_at: u64   // stale after 5 minutes → grayed out
}
```

Members write server entries to their own subkey. Clients merge all entries, filter by game_id. Stale entries (heartbeat > 5 minutes) are dimmed. No governance needed — this is community-contributed data, like player-built structures in the SSS.

### Game Time Tracking

Local SQLite tracking when `game_detect` sees a game process running. Optionally published as `game_hours: HashMap<game_id, u64>` in MemberPresence. Per-game privacy: user chooses which games to share playtime for.

### Screenshot Sharing

Standard file sharing (chunked app_call) with game context metadata: `GameScreenshot { game_id, game_name, timestamp, hash, size }`. Displayed in a dedicated gallery view per channel or per game.

---

## 19. The Strand Relay Network (Social Topology as Infrastructure)

In Death Stranding, the Chiral Network doesn't grow because Sam gets paid. It grows because every terminal he connects makes the WHOLE network more useful — for him and everyone else. Roads he builds carry other porters' cargo. Structures other players build help him cross rivers. Ropes he leaves help strangers climb cliffs. Likes on structures protect them from timefall. The incentive IS the network effect. No payment. Mutual benefit.

The Strand Relay Network applies this principle to Veilid connectivity. Friends who opt in to relay for each other create a faster, more resilient network for everyone in their communities.

### The Problem

To reach Bob, Alice needs his `route_blob`. Routes refresh every 120 seconds. If Bob's route dies and he hasn't re-published to the DHT yet, Alice falls back to DHT reads (200-500ms Kademlia lookup) or waits for a watch callback (1-30 seconds). During this window, messages to Bob through gossip fail silently — they're fire-and-forget.

Bob's friends already have live connections to Bob. They've exchanged routes directly. They're the fastest, freshest path to Bob. But exposing WHICH friends Bob has is a privacy violation.

### The Strand: Opaque Relay Routes

Veilid private routes are opaque. A `route_blob` can be imported and sent to without knowing who created it. This is the strand — an anonymous lifeline. You grab it. You don't see who's at the other end.

When Carol (Bob's friend) opts in to relay for Bob:

1. Carol creates a DEDICATED relay route — `new_private_route()` — separate from her personal route, separate from her community presence routes. Cannot be correlated with Carol's identity.
2. Carol delivers the relay route_blob to Bob via their existing direct connection (app_call, encrypted).
3. Bob publishes the blob in his personal **relay record** — a DFLT record owned by Bob's friend profile key.

```
RelayRoster {
  entries: Vec<RelayEntry {
    relay_route_blob: Vec<u8>,  // opaque — can't identify the friend
    expires_at: u64,
  }>,
}
```

The relay record key is derived from Bob's friend profile key (deterministic, discoverable by anyone who knows Bob's profile). The record contains only opaque route_blobs. The number of entries can be padded with dummy routes pointing to Bob himself to obscure the true relay count.

### Relay Message Flow

```
  Alice wants to reach Bob (direct route stale)
    │
    ├─ 1. Read Bob's relay record (cached locally or watched)
    │
    ├─ 2. Pick a relay entry (random selection)
    │     import_remote_private_route(relay_route_blob)
    │
    ├─ 3. Encrypt payload to Bob's public key
    │     RelayEnvelope {
    │       for_key: bob_public_key,
    │       ciphertext: encrypt(payload, bob_pubkey),
    │       reply_route: Option<alice_ephemeral_route>,
    │     }
    │
    ├─ 4. app_message(relay_route_id, relay_envelope)
    │
    │     ───── relay friend receives ─────
    │
    ├─ 5. Friend sees for_key matches Bob
    │     Friend forwards ciphertext to Bob via app_message
    │     Friend CANNOT read ciphertext (encrypted to Bob)
    │     Friend does NOT know who Alice is (safety route)
    │
    ├─ 6. Bob decrypts with private key
    │     reply_route → Bob can respond directly to Alice
    │
    └─ 7. Subsequent messages flow directly (routes exchanged)
```

### Privacy Guarantees

**Alice sees:** Opaque relay route_blobs. Cannot identify which friends they belong to. Even if Alice is also friends with Carol, she can't match Carol's personal route to Carol's relay-for-Bob route — they're separate private routes with different blobs.

**The relay friend sees:** A RelayEnvelope addressed to Bob's public key. They know it's for Bob (they signed up to relay for him). They can't read the ciphertext. They don't know Alice's identity.

**An observer sees:** Traffic flowing through private routes. Standard Veilid behavior. Indistinguishable from normal traffic.

**Bob's friends list is NEVER exposed.** The relay record contains only opaque blobs. The number of entries reveals nothing meaningful (padded with dummies).

### Presence Caching (Social CDN)

The relay network isn't just for message delivery. Friends with active connections to Bob have FRESH presence data — game status, voice channel, route_blob — received directly, not through the DHT.

```
  Alice → relay friend: StatusRequest { for_key: bob_pubkey }
  Friend (has fresh data from direct connection to Bob):
    → StatusResponse { presence: bob_current_presence }
  // No round-trip to Bob. No DHT read. Friend served from local cache.
```

This makes presence resolution faster than a DHT read. The social graph becomes a caching layer. When Alice clicks on Bob's name to see if he's playing Halo, the response comes from Bob's nearest online friend in ~30-50ms instead of a 200-500ms DHT lookup.

### The Incentive: Mutual Aid, Not Payment

The strand contract is reciprocal. When you relay for your friend:

- **Your friend stays reachable** → your messages to them deliver faster too
- **Your friend's communities benefit** → richer communities that you're also in
- **When YOUR route goes stale** → your friends relay for you
- **The more friends who relay** → the more resilient everyone's connectivity
- **Your community's voice calls** → lower latency because route recovery is faster

There is no payment. There is no token. There is no reward system. The reward is the network itself getting better for you. Every strand you leave is a rope someone else grabs. Every rope they leave is a ladder you climb.

The opt-in UI: "Help [friend_name] stay connected (relay their traffic when you're online)." One toggle. The relay carries encrypted blobs your device can't read. The bandwidth cost is trivial — a chat message is ~1KB. Like leaving a rope anchor in the game — costs you nothing, saves someone else.

### Latency Impact

| Scenario | Without Strand Relay | With Strand Relay |
|----------|---------------------|-------------------|
| Direct route alive | 50-150ms (gossip) | 50-150ms (no change) |
| Direct route stale | 200-500ms (DHT fallback) | 60-100ms (friend relay) |
| Presence lookup | 200-500ms (DHT read) | 30-50ms (friend cache) |
| Route recovery | 1-30s (watch callback) | Immediate (friend has live route) |

For gaming communities where friends are usually online together, stale routes almost never cause visible latency spikes. The social topology absorbs the failure.

### The Strand Equations

**Reaction-Diffusion (Tag 1):** The relay network IS local processing (friend caches Bob's status) with epidemic spread (Alice's request diffuses through the friend network). No central broadcaster.

**Entanglement (Tag 5):** Bob's relay record and his friends' relay routes are entangled — created independently on separate devices, correlated without communication. The friend creates a route. Bob publishes the blob. Alice uses it. Three nodes. No shared state beyond the opaque blob.

**Einstein (Tag 4):** The social graph (matter) shapes the relay topology (spacetime). The relay topology shapes message delivery (how matter moves). More friends = better connectivity = more people want to join = more friends. The feedback loop.

---

## 20. Mutual Aid Infrastructure (Every Node Builds the Network)

The Strand Relay Network (§19) applies mutual aid to connectivity. This section extends the same principle to every other operation in the architecture. In Death Stranding, porters don't just deliver cargo — they build roads, place bridges, stock shared lockers, erect timefall shelters, and string ziplines. Each structure costs the builder almost nothing but benefits every porter who passes through. The Chiral Network grows because individual contributions compound.

Every pattern below uses existing Veilid primitives. No new infrastructure. No special nodes. Each node chooses to do a small amount of extra work that benefits the whole community.

### 20.1 Road Building: Record Warming

**DS parallel:** Roads are the most expensive structure but free for everyone. They decay under timefall unless porters use them — usage IS maintenance.

**Veilid behavior:** The DHT refreshes records that are actively read. Records nobody reads fall off the caching layer. A community channel nobody's read in a week has cold records — the first reader pays a latency penalty while the DHT re-fetches.

**Mutual aid:** When your client is open but idle (you're in-game, away, or just not chatting), your node cycles through ALL community records with low-priority `get_dht_value` reads every 5 minutes. Not just your active channels — governance, registry, every channel. You're not reading messages. You're paving the road.

The existing `start_dht_keepalive()` already touches subkey 0 of the manifest every 300 seconds. The expansion: cycle through all community records, staggered to avoid burst. 20 channels + governance + registry = 22 reads per cycle = ~4.4KB every 5 minutes. Negligible bandwidth. Every touch keeps the record warm on DHT caching nodes for whoever needs it next.

**Storage IS the vote (Design Principle 4).** Popular channels get warmed by many members. Dead channels get warmed by fewer. The DHT naturally allocates more resources to frequently-accessed records. Record warming by idle members is the like system — every read is a vote for persistence.

### 20.2 Shared Lockers: History Advertisements

**DS parallel:** Shared lockers let porters deposit cargo for strangers. You drop off what you don't need. Someone else picks it up. You never meet.

**The problem:** When messages are trimmed from a member's DHT subkey (page trimming at ~30KB), they still exist in SQLite on every node that received them. A new member joining after the trim can't read old messages from the DHT. They need to get them from a peer — but which peer has the deepest history?

**Mutual aid:** Each member advertises their local history depth in MemberPresence:

```
history_ranges: Vec<HistoryRange {
  channel_id: [u8;16],
  oldest_lamport: u64,
  newest_lamport: u64,
}>
```

~50 bytes per channel. 20 channels = 1KB. Fits in the existing subkey budget. This tells other members: "I have #general from lamport 500 to lamport 12000."

When a new member joins and reads channel subkeys (getting only recent messages from the DHT), they scan presence subkeys for history_ranges, identify peers with the deepest history, and request catchup via app_call. No coordination. The system self-organizes from passive advertisements.

Members who've been in the community longest naturally have the deepest history. They're the shared lockers — silently holding cargo for newcomers who haven't arrived yet.

### 20.3 Bridge Building: Watch Relay

**DS parallel:** Bridges connect disconnected areas. They don't carry cargo — they enable faster traversal.

**The problem:** `watch_dht_values` has limited slots. `public_watch_limit` for non-writers, `member_watch_limit` for SMPL members. When a community grows past the watch slots on a record, some members can't get real-time notifications. They fall back to `inspect_dht_record` polling every 60 seconds — a window where they miss updates.

**Mutual aid:** A member who HAS a watch on a channel record relays change notifications to members who couldn't get a slot:

```
WatchRelay {
  record_key: String,
  subkeys_changed: Vec<u32>,
  timestamp: u64,
}
```

Sent via app_message to peers the relayer knows are watching this channel (from gossip peer list). The relay doesn't carry message content — just "subkeys 3 and 7 changed." The receiving peer does a targeted `get_dht_value` for only the changed subkeys.

Privacy maintained: the relay knows WHICH subkeys changed (public information — it's a ValueChange notification) but doesn't forward the content. The receiving peer reads the content directly from the DHT.

The member with the watch slot is a bridge — not carrying the cargo, but creating a path that lets others cross the gap between real-time and 60-second polling.

### 20.4 Timefall Shelters: Bootstrap Bundles

**DS parallel:** Timefall shelters protect anyone nearby without the builder being present. The shelter exists as infrastructure.

**The problem:** Joining a community is the most latency-intensive operation. A new member needs to: read governance (all subkeys), read registry (occupied subkeys), open all channel records, fetch MEK, build the gossip peer list. That's 30+ DHT operations in the first seconds — the exact bootstrap congestion that caused v1.0 connection exhaustion.

**Mutual aid:** When a new member joins, instead of 30 independent DHT reads, they send a `BootstrapRequest` to any online peer. The responding peer constructs a single-response bundle:

```
BootstrapBundle {
  governance_snapshot: Vec<(subkey, data, seq)>,
  registry_snapshot: Vec<(subkey, data, seq)>,
  channel_keys: Vec<(channel_name, record_key)>,
  current_mek: EncryptedMEK,  // wrapped to joiner's pubkey
  peer_routes: Vec<(pseudonym, route_blob)>,
  bundle_timestamp: u64,
}
```

Delivered in a single app_call response. One round trip instead of 30. The joiner still VERIFIES everything independently (reads governance to confirm snapshot, checks all signatures) but uses the bundle as a fast starting point.

The shelter builder doesn't know who'll use it. Any online member can serve a BootstrapBundle. The inviting member is the natural first source (they sent the invite, they're online), but any community member works. The shelter exists as a capability of every node.

### 20.5 Ziplines: Gossip Topology Optimization

**DS parallel:** Ziplines are fast-travel between specific points. They help porters who need THAT specific route.

**The current state:** `gossip_forward()` fans out to all D peers equally. But some peers are better connected — high uptime, fast routes, reliable forwarding. Others have flaky mobile connections.

**Mutual aid:** Each client tracks delivery metrics per gossip peer in local SQLite:

```
gossip_peer_stats {
  pseudonym: TEXT,
  messages_forwarded: INTEGER,
  delivery_failures: INTEGER,
  avg_response_ms: REAL,
  last_success: INTEGER,
}
```

Over time, the gossip fan-out becomes weighted — prioritize peers who reliably forward, use others as backup. The mesh self-optimizes. "Ziplines" emerge organically between the most reliable nodes. No coordination needed. Each node independently discovers the best paths through the mesh.

This requires zero protocol changes. It's a client-side optimization to the existing `gossip_forward()` — sort the peer list by reliability before iterating.

### 20.6 Porter Cargo Chains: MEK Relay via Gossip

**DS parallel:** Porters carry cargo through dangerous territory for each other. The route IS the service.

**The problem:** A returning member needs the current MEK. They broadcast `RequestMEK` via gossip. But what if their direct gossip peers are all from segment 2 and the MEK rotator was in segment 1? Or what if their peers have stale MEK caches?

**Mutual aid:** `RequestMEK` is specified as a gossip envelope type with the same TTL+dedup+forward semantics as chat messages. It flows through the mesh — if your direct peers don't have the current MEK, they forward your request to THEIR peers. The request propagates until it reaches someone who has it. That someone responds via app_call directly to the requester's route (included in the request).

```
CommunityEnvelope::RequestMEK {
  requester_pseudonym: String,
  requester_route_blob: Vec<u8>,
  current_generation: u64,  // what I have, so responder knows I need newer
  ttl: u8,
}
```

The mesh IS the relay chain. Every node that forwards a RequestMEK is a porter carrying cargo (the request) through territory (the network) that the requester can't traverse directly.

### The Compound Effect

Each pattern is small individually. Together they transform the network:

- **Idle member** warms records (roads) + advertises history (shared locker) + relays watches (bridge) = a sleeping node that's still contributing
- **New member** receives a bootstrap bundle (shelter) + reads history from deepest peer (shared locker) + gets real-time watch relays (bridge) = joins in seconds, not minutes
- **Active member** gossips through optimized paths (ziplines) + relays for friends (strands) + forwards MEK requests (porter) = the mesh gets faster the more people use it
- **Every `get_dht_value`** is a like. Every watch relay is a bridge. Every forwarded message is a porter delivery. The network grows because using it IS building it.

No tokens. No payment. No reputation scores. The incentive is structural — the network gets better for YOU when you contribute to it, because you're IN the network. Build a road and you drive on it too.

---

## 21. Spam Defense Without a Server

No centralized AutoMod. Defense is local and distributed, like Sam handling MULEs independently.

**Gossip rate limiting:** Each peer independently tracks message rate per sender pseudonym. If sender exceeds threshold (configurable, default 10 messages/second), drop subsequent gossip from that sender for a cooldown period. Every node runs the same check independently. A spammer sees their burst swallowed by the mesh.

**Gossip amplification prevention:** The rate limiter operates BEFORE gossip forwarding. In the existing `handle_gossip_envelope` flow, between dedup check and `gossip_forward()`, a per-sender rate counter is checked. If the sender is over rate, the message is still processed locally (so the receiver can see it for banning purposes) but NOT forwarded. This contains amplification to one hop — the spammer's messages reach their direct peers but don't propagate through the mesh. ~15 lines of code in the existing forward path:

```
let sender_over_rate = state.gossip_rate_limiter.check(&signed.sender_pseudonym);
if signed.ttl > 0 && !is_private && !sender_over_rate {
    gossip_forward(state, community_id, &signed);
}
```

**DHT natural throttle:** SMPL subkey writes are rate-limited by Veilid's own DHT write controls. A spammer can flood gossip (fire-and-forget app_message) but can't flood the DHT (rate-limited set_dht_value). Since SMPL write is the primary delivery path, the persistence layer is inherently protected.

**Ban response:** Any admin writes BanEntry governance entry. Propagates through CRDT merge. All honest clients ignore banned member's messages from that point forward. The window between spam start and ban is covered by local rate limiting — the burst is absorbed, then governance catches up.

**Message validation:** Every gossip message is signature-verified (already implemented). Forged sender pseudonyms are rejected. A banned member can't impersonate someone else.

**Honest tradeoff:** No instant automated content filtering like Discord's AutoMod. Content rules are advisory — clients can implement local keyword filters, but there's no server-side enforcement. The defense stack is: local rate limiting → gossip dedup → amplification containment → signature verification → governance bans.

---

## 22. Mobile Push Notifications (VICE Escalation Pattern)

When the app is backgrounded on iOS/Android, Veilid can't maintain connections. The three-path model handles message delivery (SMPL catchup on return), but the user doesn't KNOW messages arrived until they open the app.

**Chiral Network parallel:** VICE doesn't eliminate NAT — it works THROUGH it. Direct when possible, hole-punched behind NAT, relayed as last resort. Push follows the same escalation:

**Tier 1 — Direct (app in foreground):** Veilid connection alive. Gossip delivers immediately. Watches fire. No push needed.

**Tier 2 — Background fetch (OS-granted):** iOS/Android grant periodic background execution. The app briefly connects to Veilid, calls `inspect_dht_record` on watched channel records, fires a local notification if new data detected. No external relay. Unreliable — OS can defer or suppress background execution.

**Tier 3 — Push relay (opt-in fallback):** A headless veilid-server that watches your community records and sends push via FCM/APNs. This is APAS — an automated participant providing mutual aid, not special infrastructure.

**Push relay design:**

1. Relay opens your community records as a reader (public watches, no write access).
2. On `VeilidUpdate::ValueChange` for any watched channel subkey, sends push to your device.
3. Push payload: community_id, channel_id, "new message" flag ONLY. No content. No sender. The relay never sees decrypted messages.
4. Your device token is shared with the relay via encrypted app_call. Stored in memory only.
5. Your chosen relay is published in MemberPresence as `push_relay_route: Option<Vec<u8>>`.

**Privacy tradeoff:** The relay learns which communities you're in and when messages arrive (timing metadata). This MUST be opt-in with clear UI disclosure.

**Anyone can run one.** No single push relay. Self-host, use community-provided, or use a Rekindle-operated default. Multiple relays for redundancy. The relay is not a special node — it's a Veilid peer.

---

## 23. File Sharing (Lost Cargo Pattern)

**Chiral Network parallel:** Lost cargo. Dropped packages persist in the world for anyone to find. Multiple porters can carry the same cargo. Chiralium coating protects from timefall.

**Upload:** Alice shares a file → chunks into ≤28KB pieces → SHA-256 per chunk + whole file → writes `AttachmentOffer { hash, filename, size, mime_type, chunk_count }` as part of her ChannelEntry::Message in her channel subkey.

**Download:** Bob requests file → sends `RequestAttachment { hash, chunk_indices }` via gossip. Alice (or ANY peer who has the file) responds via app_call with the chunks. Bob verifies chunk hashes during reassembly, whole-file hash on completion.

**Availability tracking:** Every peer who downloads a file writes `AttachmentCached { hash, available_since, lamport }` as a ChannelEntry in their channel subkey. When someone requests a file, they scan channel subkeys for AttachmentCached entries with matching hash and try those peers via app_call. More downloaders = more sources.

**Local pinning (chiralium coating):** For critical community files (rules, assets), any member marks them as "pinned locally" — indefinite caching and availability. Deliberate preservation.

**Offline availability:** If ALL peers with a cached file go offline, the file is unavailable until one returns. Honest about this. Popular files are widely cached. Niche files depend on pinning.

**Block store migration:** When Veilid's block store ships upstream, file bytes move from local peer-cache to DHT block storage. The metadata layer stays unchanged. The hash IS the address.

---

## 24. Search (Local FTS5)

E2E encryption means search is local-only. You can only search what your client has decrypted. This is a fundamental privacy tradeoff — the system cannot search what it cannot read.

**Implementation:** FTS5 virtual table in SQLite, indexed on every message insert (after decryption). Full-text search with filters:

```
search(query, filters) where filters:
  from: pseudonym_key     — messages from specific member
  in: channel_id          — messages in specific channel
  has: attachment | link | embed | mention | reaction
  before: timestamp       — messages before date
  after: timestamp        — messages after date
  mentions: pseudonym_key — messages mentioning specific member
```

**New member experience:** Search covers messages from join date forward. Pre-join history is available if peers share it during catchup (messages received via SMPL read-all-subkeys on join get indexed), but older messages that have been trimmed from DHT subkeys and not yet received are not searchable until a peer serves them.

---

## 25. Screen Sharing and Streaming

**Interim protocol:** Fragmented app_message at ~480p 15fps. Usable for "look at what I'm seeing" moments. Not sufficient for streaming gameplay at quality.

**Pipeline:** Screen capture → VP8/VP9 encode at constrained bitrate → fragment into ≤32KB chunks → MEK encrypt → app_message to voice channel participants. Receiver reassembles → decrypt → decode → display in picture-in-picture overlay.

**Long-term:** The `veilid-media` contribution to upstream (deferred Phase A: RFC, Phase C: implementation). Proposes a `media_stream()` API to Veilid maintainers. Would unlock 720p+ at 30fps while maintaining privacy routing. Benefits the entire Veilid ecosystem, not just Rekindle.

**Honest about it:** Discord started with limited video quality and improved over time. Rekindle ships with usable interim video and invests in the upstream contribution for quality improvements.

---

## 26. Community Discovery and Onboarding

### Onboarding

`OnboardingConfig` and `WelcomeScreen` are governance entry types. Any admin with MANAGE_COMMUNITY can write them. Clients show the welcome screen on first join, present onboarding questions, and assign opt-in roles based on answers.

### Discovery

**Veilid-native (primary): Self-service DHT directory.** A well-known SMPL record key (hardcoded in the client) where community admins write `DirectoryListing { bootstrap_key, name, description, member_count, tags }` to a subkey. The slot_seed for the directory is PUBLIC (published in Rekindle client source code) — any community admin can derive a keypair and list their community. Same universal schema. Plate gate scaling when directory exceeds 255 listings. Self-service, no gatekeeper.

**Web directory (supplementary):** A curated web service indexing community bootstrap keys. Centralized index but no authority — it's just pointers. Communities opt-in to listing. Useful for SEO and discoverability outside the Rekindle client.

---

## 27. Direct Messages (Chiralgrams)

**Chiral Network parallel:** Chiralgrams — private holographic communication between two endpoints. Requires both parties to have active routes.

**Design:** A DM is a 2-party SMPL record. `o_cnt: 0, members: [alice_pseudo_keypair, bob_pseudo_keypair], m_cnt: 1`. Alice writes to subkey 0, Bob to subkey 1. Both watch the record. Messages encrypted with a DM-specific key derived via X25519 ECDH between the two pseudonym keys.

**Initiation:** Alice wants to DM Bob. She reads Bob's presence subkey in the registry → gets his `route_blob`. Creates the 2-member SMPL record. Sends `DMInvite { record_key, my_pseudonym }` to Bob via app_call to his route. Bob opens the record, accepts or rejects. Record key stored in both users' local SQLite.

**Privacy:** DMs use community pseudonyms — Alice and Bob are unlinkable across communities even in DMs. The DM record key is not published anywhere except the two participants' local stores.

**Group DMs:** Same pattern with more members. 3-8 member SMPL record with o_cnt: 0. Same ChannelEntry enum as channels. Group DM initiator sends DMInvite to each participant.

---

## 28. User Blocking (Odradek Local Scan)

**Chiral Network parallel:** The Odradek's BT detection. Local scanning. Your device detects and marks threats. Other players' devices don't know your threat markers.

**Design:** Block list stored in SQLite: `blocked_pseudonyms: Vec<(community_id, pseudonym_key)>`. On every message render, check if author is in block list. If yes, don't display. On gossip receive, still process and forward (other peers may want the message) but don't emit to frontend.

**Per-pseudonym, per-community:** Because HKDF pseudonyms are unlinkable across communities, blocking is per-pseudonym per-community. Blocking "the person" across all communities is impossible by design — that would require linking pseudonyms, which breaks the privacy model.

**Persists across restarts:** SQLite storage. Block actions are local-only — not published to any DHT record. No one knows who you've blocked.

---

## 29. Unread Tracking and Read Receipts

**Unread tracking (local):** Per-channel `last_read_lamport` in SQLite. When you open a channel, set `last_read_lamport = max(lamport of all visible messages)`. Unread count = messages with lamport > last_read_lamport. Entirely local. No network traffic.

**Read receipts (optional, privacy-sensitive):** Each member CAN write `ReadReceipt { channel_id, read_up_to_lamport }` to their presence subkey. Other clients see it. This reveals when you last read each channel — a timing metadata leak. Opt-in per user in settings. Default: OFF.

**Typing indicators:** Already implemented via gossip. TypingIndicator envelope with 5-second bucketed dedup. No change needed.

---

## 30. Emoji, Stickers, and Soundboard (Chiral Printer Blueprints)

**Chiral Network parallel:** Chiral printer blueprints. Fabrication plans distributed to every terminal. Once you have the blueprint, you fabricate locally without re-downloading.

**Metadata:** Governance entry types: `EmojiCreated { emoji_id, name, hash, uploader_pseudo, lamport }`, `StickerCreated { ... }`, `SoundCreated { ... }`. Removal via `ExpressionRemoved { expression_id, lamport }`.

**Distribution:** Emoji images are small (~64KB for 128×128 animated GIF). Every member's client downloads and caches ALL community emoji automatically during join and governance sync. New emoji added later fetched when governance ValueChange arrives. Transferred via app_call from any online peer who has the cached copy.

**Cache budget:** 50 emoji at 64KB = 3.2MB. 100 stickers at 200KB = 20MB. 50 sounds at 500KB = 25MB. Total ~48MB for a large community. Manageable. Clients eagerly cache everything — emoji/stickers/sounds MUST be always-available for UX (you can't show an emoji picker that says "loading from peer...").

**Always-available guarantee:** Unlike regular file attachments (which depend on a peer being online), expression assets are eagerly replicated to every member's local cache. They behave like chiral printer blueprints — once distributed, every terminal can fabricate locally.

---

## 31. Bot API (APAS Delivery Bots)

**Chiral Network parallel:** APAS delivery bots. Automated participants doing porter work. Not special infrastructure — just nodes running automation instead of being operated by a human.

**A bot is a headless Veilid node with a community member slot.** It derives its slot keypair from the same slot_seed. Joins like any other member (self-sovereign join via InviteSecrets). Reads channel subkeys via watches. Writes responses to its own subkey. Reads governance for channel list, permissions. Can be assigned a "bot" role via governance (RoleAssignment) with restricted permissions.

**Webhook equivalent:** A bot that listens for events on an external service (GitHub, Twitch, game server) and writes messages to its channel subkey. The webhook endpoint is hosted by whoever runs the bot.

**Bot SDK (future work):** Wraps rekindle-protocol into a clean API:

```
create_bot(community_key, slot_seed) → BotContext
bot.on_message(channel_id, callback)
bot.send(channel_id, content)
bot.on_governance_change(callback)
bot.on_member_join(callback)
```

Architecturally, bots require zero protocol changes. They're just members. The SDK is an API design task.

---

## 32. Cross-Device Sync (Cuff Link Pattern)

**Chiral Network parallel:** Sam's cuff link / ring terminal syncs his status across every terminal he visits. The device carries the state; the network reflects it.

**Design:** Each user has a personal DFLT record created with their master keypair (not community-specific). Contains:

```
ReadState {
  per_community: HashMap<community_id, HashMap<channel_id, last_read_lamport>>
}
```

Both devices write to it. Both watch it. Last-writer-wins — mark-as-read on phone → phone writes → desktop sees ValueChange → updates local state.

**Sizing:** ~100 bytes per channel entry. 5 communities × 20 channels = 10KB. Fits easily in one subkey.

**Discovery:** Record key is derived deterministically from the user's master key. Same master key = same record key on any device. No out-of-band coordination needed.

---

## 33. Mentions (@user, @role, @everyone)

**Chiral Network parallel:** Odradek ping. Local scan highlights things relevant to YOU. Other players' Odradeks highlight different things.

**Format** (all clients must agree):

```
@<pseudonym_key_hex>     — mention specific member
@role:<role_id_hex>       — mention everyone with that role
@everyone                 — mention all members
```

Client parses these tokens after MEK decryption. If your pseudonym or any of your roles matches, escalate notification. The `has: mention` search filter checks for these tokens in the FTS5 index.

**@everyone permission gate:** MENTION_EVERYONE permission bit in the role bitmask. If the writer doesn't have it, receiving clients render @everyone as plain text, not as a notification trigger. Same client-side validation as all permissions.

---

## 34. Categories, Channel Ordering, and Channel Config

**Categories (Chiral Network: regional grouping on the map — display-only, no protocol effect):**

`CategoryCreated` and `ChannelCategoryAssignment` governance entries. CRDT merge: CategoryCreated is UNION, assignment is LWW per channel_id. A channel is in exactly one category (latest assignment wins). Unassigned channels go to default "uncategorized" group. Categories affect only visual grouping — not permissions, delivery, or any protocol behavior.

**Channel ordering and topic:**

`ChannelUpdated { channel_id, new_name, new_topic, new_position, lamport }` governance entry. LWW per field per channel_id. Clients take ChannelCreated as base, apply all ChannelUpdated entries in Lamport order.

**Slowmode (Chiral Network: terrain difficulty — slows you down, doesn't prevent movement):**

`ChannelConfig { channel_id, slowmode_seconds, nsfw: bool, lamport }` governance entry. Client-side send enforcement: UI grays out send button until timer expires. Receive-side enforcement optional: clients can flag messages violating slowmode. Gossip rate limiter from §19 provides a hard floor regardless — a rogue client bypassing slowmode still can't exceed 10 msgs/sec through the mesh.

**Notification settings:** Entirely local SQLite. Per-channel and per-community mute levels (all / mentions / nothing). No protocol involvement.

---

## 35. Message Forwarding (Lost Cargo to New Destination)

`ChannelEntry::Forward` with a content snapshot re-encrypted to the destination channel's MEK. Self-contained — readers don't need access to the original channel.

The forwarder decrypts from source MEK, re-encrypts to destination MEK. The original author's pseudonym is included for attribution. Since pseudonyms are per-community (HKDF unlinkable), forwarding within the same community reveals the author's identity only to members of the destination channel who are also in the same community (which they already are).

---

## 36. Polls (Likes with Named Options)

Poll defined in `ChannelEntry::Message` via `poll: Option<Poll>`. Votes via `ChannelEntry::PollVote` from each voter's own subkey. Same SSS merge pattern as Reactions — independent contributions, locally merged.

Multi-vote controlled by `poll.multi_vote`. Expiry: clients ignore PollVote entries where `timestamp > poll.expires_at`. Vote changing: latest PollVote per (voter_pseudonym, message_id) wins (LWW).

---

## 37. Scheduled Events and RSVP (Drawbridge Expedition Roster)

`EventCreated` governance entry with title, description, start/end times, optional voice channel link. Events with `end_time < now` are auto-hidden in UI. Governance entry persists (CRDT entries are never deleted) but clients filter by time.

RSVPs live in each member's MemberPresence: `event_rsvps: Vec<EventRSVP { event_id, status }>`. Clients building event detail view scan all occupied presence subkeys (same inspect + read pattern as member list) and collect RSVPs for the target event_id. Piggybacks on existing presence reads — events with zero RSVPs cost zero extra work.

---

## 38. Stage Channels (Formal Chiralgram)

Voice channel mode where only designated speakers transmit. Same voice pipeline — the difference is a permission gate on the send side.

`StageConfig { channel_id, stage_mode: bool, speakers: Vec<pseudonym>, lamport }` governance entry. Clients joining a stage channel check if their pseudonym is in `speakers`. If not, client suppresses the send pipeline (mic muted before Opus encode — zero processing cost). Receiving clients also check: audio from non-speakers is dropped at the mixer. Rogue clients transmitting without permission are silenced at every honest receiver.

**Hand-raising:** `ChannelEntry::HandRaise { channel_id, lamport }` in the member's channel subkey. Admins see it in UI and can add the member to `speakers` by writing an updated StageConfig.

---

## 39. Audit Log (The Governance CRDT IS the Log)

**Chiral Network parallel:** Corpus Database. A replicated record of everything that's happened.

No separate audit log record. The governance record itself is tamper-evident history. Every entry is Ed25519 signed, Lamport-ordered, attributable to a specific member.

**Client audit log view:** Read all governance subkeys → collect all entries → sort by Lamport → filter by type and actor → display chronologically. Follow GovernanceOverflow pointers for complete history.

**Advantages over Discord's audit log:** Tamper-evident (signatures, can't be altered). Complete (no retention limits, CRDT entries are never dropped). Decentralized (no single party controls the log).

---

## 40. Community Icon and Banner (Eager Peer-Cache)

`CommunityMeta` governance entry contains `icon_hash` and `banner_hash`. Actual image bytes (~256KB icon, ~1MB banner) distributed via same eager peer-cache as emoji (§28). On governance sync, client detects new hash → requests from any online peer → caches locally. Fetched once, cached indefinitely, re-fetched only when hash changes.

---

## 41. Display Names (Self-Sovereign)

`display_name: Option<String>` in MemberPresence. Each member sets their own community-specific nickname by writing to their own presence subkey. No governance action needed — your name is your choice. The pseudonym_key is the stable cryptographic identity; display_name is the human-readable label that can change freely.

---

## 42. Honest Tradeoffs vs Discord

| Feature | Discord | Rekindle | Tradeoff |
|---------|---------|----------|----------|
| Message delivery | Instant (server push) | ~50-150ms same-region via gossip. Stale routes recover in 60-100ms via strand relay (vs 200-500ms DHT fallback) | Comparable to Discord for same-region gaming communities |
| Search | Server-side, all history | Local FTS5, from join date | Privacy: can't search what you can't decrypt |
| File availability | CDN, always available | Peer-cached, requires online peer | Files depend on community activity |
| Push notifications | Built-in, instant | Opt-in relay, timing metadata leak | Privacy tradeoff is explicit and user-chosen |
| Spam/AutoMod | Server-side, instant | Client-side rate limit + governance ban | Brief burst before ban; no content filtering |
| Message deletion | Server-side, permanent | Client-side tombstone, not guaranteed | Peers may retain; honest UX about this |
| Video quality | Up to 4K streaming | ~480p 15fps interim | Improving via upstream veilid-media contribution |
| Bot ecosystem | Massive, mature | Headless member nodes, SDK planned | Architecturally clean (bots = members), SDK is future work |
| Monetization | Nitro, server boosts, ads | None — FOSS, no payment model | Free forever. Development funded by the developer. |
| Max server size | 500K+ members | 255 per segment (scaling via plate gates) | Fractal scaling works but adds read complexity |

**What Rekindle offers that Discord cannot:**

| Feature | Why Discord can't do this |
|---------|--------------------------|
| No IP logging | Veilid private routes. Discord knows your IP. |
| No data mining | E2E encrypted. Discord reads everything for ads/safety. |
| No deplatforming | No server to shut down. Discord bans entire communities. |
| Community survives creator leaving | Flat CRDT governance. Discord servers die with owner. |
| Cross-community identity unlinkable | HKDF pseudonyms. Discord tracks you across servers. |
| Game join from presence | Direct game connection. Discord's is limited and game-dependent. |
| Community-owned infrastructure | Members run the network. Discord owns the servers. |
| Open protocol | Anyone can build clients. Discord's API is proprietary and revocable. |
| Free forever, no ads | FOSS with no payment model. Discord monetizes via Nitro, boosts, and ads. |

---

## 43. Design Principles

1. **No node above another.** No coordinator. No owner. No privileged node. Admin is a permission set in the CRDT, not a structural capability. Every member has the same SMPL subkeys.

2. **One equation everywhere.** SMPL { o_cnt: 0, m_cnt: 1, 255 slots }. Same schema for registry, governance, and channels. Same seed. Same derivation. The Q-pid activates every terminal the same way.

3. **DHT is primary, gossip is secondary.** The SMPL channel record is the road infrastructure. Gossip is Sam running cargo — fast but unreliable.

4. **Storage IS the vote.** Data that peers use persists. Data nobody requests decays. No separate reputation system needed.

5. **Privacy is a stamina budget.** Every safety hop costs latency. Apply stealth proportional to sensitivity: full safety for chat, no safety for voice.

6. **Assume everything degrades.** Routes die. Watches fail. DHT records go stale. Build three paths and let any one succeed.

7. **The reader validates, not the writer.** Permission enforcement is client-side. Every node independently merges governance entries and checks permissions. A rogue writer's entries are ignored by every honest reader.

8. **Governance is replaceable; infrastructure is not.** Channel SMPL records and the member registry survive any governance transition. The messages ARE the community. Governance can fork; the conversation continues.

9. **Grow organically, not by plan.** Plate gate segments created when needed. Channel segments created lazily. No batch orchestration. The network grows from individual contributions.

10. **All roads through Veilid.** No external transport. No IP leakage. VICE handles NAT. The terrain is hostile but the transport layer handles it.

11. **Honest about tradeoffs.** Deletion is a request, not a guarantee. Search is local-only. Push requires an opt-in relay. File availability depends on peers. Say what the system can and cannot do. Don't promise Discord's UX on P2P infrastructure.

12. **Mutual aid is the incentive.** No tokens. No payments. No reward systems. The reward is the network getting better for you. Relay for your friend → your messages deliver faster. Cache community emoji → the picker loads instantly for everyone. Pin a file → the whole community has it. Leave a rope. Someone grabs it. Later they leave a ladder. You climb it.
