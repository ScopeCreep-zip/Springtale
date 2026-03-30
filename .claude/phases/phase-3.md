# Phase 3 — Veilid Mesh

> Source: `docs/current-arch/ARCHITECTURE.md` §3, §6.3, §7, §14.4
> Source: `docs/current-arch/rekindle-architecture.md` (full document)
> Depends on: Phase 2b complete AND rekindle-protocol production-stable

## Goal

Swap `HttpTransport` for `VeilidTransport`. Agents discover peers via DHT.
Connector registry becomes distributed. No central coordination server.
Bots become headless Veilid community members. E2E encrypted P2P AI chat.

**This phase is gated on Veilid maturity.** As of 2026-03-28, Veilid is
in active beta (VeilidChat v0.4.x). Phase 3 does not have a timeline —
it ships when rekindle-protocol is production-stable.

## VeilidTransport

Replace `HttpTransport` with `VeilidTransport` implementing the same
`Transport` trait. One config change, one new impl.

**How it maps to the three-path delivery model (from rekindle-architecture.md):**

```
Transport::send(to, msg)
├── PATH 1: SMPL Write (durable)
│   Write to sender's own subkey in shared SMPL record.
│   Retry with exponential backoff. Persists for offline catchup.
│
├── PATH 2: Gossip (fast)
│   import_remote_private_route(receiver.route_blob)
│   app_message(Target::RouteId(route_id), signed_bytes)
│   Fire-and-forget. Sub-second to online peers.
│
└── PATH 3: Watch + Inspect (consistency)
    watch_dht_values() on shared SMPL records.
    inspect_dht_record() every 60s for gap detection.

Transport::recv()
    Merge from all three paths -> dedup by message_id
    -> sort by Lamport timestamp -> return next message.
```

**Semantic impedance (flagged in audit):** The `Transport` trait is
point-to-point (`send(to, msg)`) but Rekindle channels are pub/sub
(write to own subkey, all members read). Resolution: for bot DM mode,
`send(to, msg)` maps directly to SMPL write + gossip to one peer. For
community channel mode, `connector-rekindle` handles the pub/sub
semantics above the transport layer — the Transport trait is used for
the underlying Veilid operations, not for channel semantics.

**Identity mapping:** `Transport::node_id()` returns the Veilid-format
`TypedKey` wrapped in `NodeId`. The Ed25519 keypair from springtale-crypto
is the master key. HKDF derives per-community pseudonyms from it.

**Important:** springtale-crypto needs HKDF pseudonym derivation added for
Phase 3. This was flagged in the audit as a cross-doc gap — the Phase 1b
bot_id module stores the keypair but doesn't derive pseudonyms yet.

**NAT traversal:** Veilid's VICE layer handles everything transparently.
No application-level ICE/STUN/TURN.

**Research needed:** Veilid API stability (track gitlab.com/veilid/veilid
releases). `veilid-core` crate API for: `api_startup()`, `routing_context()`,
`new_private_route()`, `app_message()`, `watch_dht_values()`,
`inspect_dht_record()`, `create_dht_record()` with SMPL schema.

## connector-rekindle

The primary Phase 3 use case. E2E encrypted DM with your Springtale bot
over Veilid. No server. No phone number. No metadata.

**Two modes:**

**DM mode (Chiralagram):**
1. User scans QR code in Rekindle client (bot's public key + route_blob)
2. Rekindle client creates 2-party SMPL record (`o_cnt: 0, m_cnt: 1, 2 members`)
3. ECDH between user and bot pseudonym keys -> DM key (X25519)
4. Note: Ed25519-to-X25519 key conversion required (flagged in audit)
5. User writes encrypted message to subkey 0
6. Bot watches record, decrypts, routes through pipeline
7. Bot writes response to subkey 1

**Community mode (APAS):**
1. Bot joins community via InviteSecrets (same flow as human join)
2. Bot claims a member slot, derives slot keypair from slot_seed
3. Bot watches channel subkeys for mentions/commands
4. Bot writes responses to its own subkey in channel records
5. Community members see bot responses in normal chat flow

**Security concern (from audit):** The Bot SDK in rekindle-architecture.md
exposes `slot_seed` to the bot. The slot_seed allows deriving ALL 255
member keypairs. A compromised bot could impersonate any member. Resolution:
`connector-rekindle` should accept a single slot index + derived keypair,
not the raw slot_seed.

**Modules:**
- `dm.rs` — create/accept DM SMPL records, ECDH key derivation, encrypt/decrypt
- `channel.rs` — join community channels, watch subkeys, write responses
- `presence.rs` — write MemberPresence to registry (online/offline, current task)
- `triggers.rs` — `DmReceived`, `ChannelMention`, `GovernanceChange`
- `actions.rs` — `SendDm`, `SendChannelMessage`, `UpdatePresence`
- `pairing.rs` — QR code / deep link pairing flow

## Distributed Connector Registry

Connector registry migrates from local database (SQLite/PostgreSQL) to
Veilid DHT records.

**How to build:**
- `springtale-store::backend::trait_` already has `StorageBackend` trait
- Add `RegistryBackend` trait abstraction in springtale-connector
- `LocalRegistryBackend` wraps existing SQLite/PostgreSQL queries (Phases 1-2)
- `DhtRegistryBackend` wraps Veilid DHT read/write (Phase 3)
- `runtime::boot` selects backend based on configured transport
- Migration: `springtale-cli registry migrate --to-dht`

**DHT registry structure:** Single SMPL record with universal schema
(`o_cnt: 0`, 255 member subkeys). Each connector author writes their
signed manifest to their own subkey. Readers verify Ed25519 signatures.

## Rekindle Bot Bridge

`springtale-bot::bridge::rekindle` wraps rekindle-protocol into the
springtale-bot API.

```rust
pub struct RekindleBotBridge {
    veilid_node: VeilidNode,     // placeholder type — actual type TBD
    community_key: TypedKey,
    my_slot: u8,
    my_keypair: Ed25519Keypair,  // derived from slot_seed + index
}

impl RekindleBotBridge {
    pub async fn join(invite: InviteSecrets) -> Result<Self>;
    pub async fn resume(community_key: TypedKey, slot: u8) -> Result<Self>;  // restart path
    pub async fn on_message(&self, channel_id: ChannelId, handler: AsyncHandler);
    pub async fn send(&self, channel_id: ChannelId, content: &str) -> Result<()>;
    pub async fn on_governance_change(&self, handler: AsyncHandler);
    pub async fn on_member_change(&self, handler: AsyncHandler);
}
```

**Note:** `on_message` handler must be async (flagged in audit — original
design used `impl Fn(Message)` which can't do async work). Also needs a
`resume()` path for restart (flagged as missing bot lifecycle management).

## HKDF Pseudonym Derivation

Add to springtale-crypto for Phase 3:

```rust
pub fn derive_pseudonym(
    master_key: &Secret<SigningKey>,
    community_id: &[u8],
) -> Ed25519Keypair {
    // HKDF-SHA256 with master_key as IKM, community_id as info
    // Produces unlinkable per-community identity
}
```

Master key NEVER leaves the device. Pseudonym in Community A cannot be
correlated with pseudonym in Community B by any observer.

## Known Risks and Open Questions

From the architecture audit:

1. **Veilid maturity** — still beta. No production apps at scale. SMPL
   performance not publicly benchmarked.
2. **Transport semantic mismatch** — point-to-point trait vs pub/sub channels.
   Resolved by handling pub/sub in connector-rekindle, not in Transport.
3. **CRDT merge rules** — several defects fixed in audit but the merge engine
   is estimated at weeks of work, not days. Test thoroughly.
4. **Plate gate scaling** — slot derivation mapping between logical indices
   and per-record indices needs careful implementation.
5. **Gossip TTL** — default not specified in rekindle-architecture.md. Must
   decide before implementation.
6. **Slot reclamation** — no mechanism for freeing departed member slots.
   255-member limit is permanent per segment without this.

## Not In Phase 3

- No statistical sentinel baseline (deferred)
- No trajectory analysis (deferred)
- This is the final planned phase. Future work is feature additions, not new phases.
