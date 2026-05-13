# ADR 0009: Veilid for the Phase 3 P2P transport

**Status:** Accepted
**Date:** 2026-03-28

## Context

Phase 3 of the roadmap is P2P — let Springtale daemons talk to each
other without a central server, without trusting an IP-revealing
infrastructure, without phone numbers or email addresses. The target
use case is E2E encrypted AI chat / coordination between people whose
threat model includes "the server operator is compromised".

The candidate networks:

- **Veilid** — Cult of the Dead Cow's project. Pure P2P, no central
  servers, no IP leak, DHT for storage, baked-in cryptography that
  treats privacy as a baseline.
- **libp2p** — IPFS's protocol stack. Widely adopted, modular.
- **Tor onion services** — well-understood anonymity properties.
- **Reticulum** — niche but appropriate for our threat model.
- **Custom on top of QUIC + WireGuard** — roll our own.

## Decision

Target Veilid for Phase 3. `crates/springtale-transport/src/veilid/`
holds a `VeilidTransport` stub (every method returns
`TransportError::NotConnected`) that compiles against the current
Veilid API. We'll wire it for real when Veilid's upstream is stable
in production.

## Consequences

Positive:

- Veilid is purpose-built for the threat model we care about. It was
  designed by people working on tools for activists and journalists.
- DHT primitive gives us a connector-registry layer for free (Phase 3
  scope: distributed connector discovery).
- No central server, no relay, no exit node. The mesh is the network.
- Pseudonymity at the protocol level, not bolted on.
- Pure Rust, fits our toolchain.
- HKDF-derived per-community pseudonyms prevent cross-community
  identity correlation.
- Cap'n Proto for the wire format means cheap to verify, fast to
  decode.

Negative:

- Veilid is still pre-1.0 itself. The API has shifted. Our stub
  exists to compile against the current shape; we'll have to bring
  it forward as upstream stabilises.
- Smaller community than libp2p. Less tooling, fewer eyes.
- Performance characteristics for high-frequency cooperation traffic
  are untested. We'll need to benchmark before promising cooperation
  over Veilid (vs over LAN / VPN).
- We can't ship Phase 3 until Veilid is stable. Realistically: 2026
  late or 2027.

Locks in:

- The Phase 3 transport choice. Switching after we ship would mean
  breaking existing federations.
- Some of our cooperation primitives (mental model persistence, cross-
  formation gossip) are designed knowing they'll one day ride Veilid's
  DHT. The data shapes assume eventual consistency, signed records,
  bounded subkey writers.

## Alternatives considered

### Option A — Veilid (picked)

Pros and cons enumerated above.

### Option B — libp2p

Pros: mature, large community, modular.
Cons: identity model is weaker for our use case (peers know each
other's PeerIDs trivially). Bootstrapping requires known bootstrap
nodes, which become central points. Bandwidth profiles are tuned for
IPFS workloads (file distribution), not for the small-payload
high-frequency traffic of cooperation.

Why we didn't pick it: identity model. We'd be layering anonymity on
top of libp2p, which is the wrong order.

### Option C — Tor onion services

Pros: well-understood anonymity. Easy to use.
Cons: requires running Tor itself. Latency is high; cooperation
tick-rate is incompatible with multi-hop circuit latency. Tor exit
nodes are a separate threat model.

Why we didn't pick it: latency. The cooperation tick fires multiple
times per second; Tor circuits are 300+ms hops.

### Option D — Reticulum

Pros: cleanly designed for adversarial environments. Built-in
identity model that matches our needs.
Cons: very small community. Few existing tools. Network effects
are real — fewer peers means smaller anonymity set.

Why we didn't pick it: smaller anonymity set. Veilid's larger
community gives us a bigger crowd to disappear into.

### Option E — Custom QUIC-over-WireGuard

Pros: full control.
Cons: we'd be inventing our own DHT, our own identity model, our own
NAT traversal. Years of work to get to where Veilid already is.

Why we didn't pick it: not where our value-add lives. We're an
automation platform, not a networking research project.

### Option F — Drop Phase 3 entirely

Pros: ship 2a and 2b, call it done.
Cons: half the threat model we set out to address requires P2P.
"Local-first" without "device-to-device" is a partial solution.

Why we didn't pick it: too important to drop. We'd rather have a
stub today and a real implementation later than no plan at all.

## References

- `crates/springtale-transport/src/veilid/stub.rs` — current stub
- `docs/intended-arch/rekindle-architecture.md` — Rekindle protocol
  (the Veilid-native chat app Springtale's transport will target)
- [Veilid](https://veilid.com)
- [Rekindle](https://github.com/ScopeCreep-zip/Rekindle)
- Related: ADR 0007 (Axum) — `HttpTransport` is the Phase 2a interim;
  `VeilidTransport` is the Phase 3 successor
