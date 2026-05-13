# ADR 0002: Wasmtime for the connector sandbox

**Status:** Accepted
**Date:** 2026-03-28

## Context

Community connectors run in a WASM sandbox. We need a runtime that
gives us:

- **Fuel metering** — a deterministic instruction budget per
  invocation. Required for time-bounded execution under adversarial
  code.
- **Memory limits** — bounded heap per instance.
- **Wall-clock timeout** — independent of fuel, terminate runaway
  loops even if they're not burning instructions fast.
- **No native code generation we don't control.** AOT and JIT both
  are acceptable; we don't want a runtime that loads arbitrary code
  outside its sandbox.
- **Component Model** support so we can target stable WIT worlds.
- **Pure Rust** if possible — for `forbid(unsafe_code)` discipline in
  the rest of the stack.
- **Active maintenance** by an organisation that takes security
  seriously (because every CVE in the runtime is a CVE in our
  sandbox).

## Decision

Use [Wasmtime](https://wasmtime.dev) (currently v43). All community
connectors are loaded through `crates/springtale-connector::wasm`.

## Consequences

Positive:

- Fuel metering (`epoch_interruption`) gives us deterministic
  per-invocation budgets.
- Wasmtime is the Bytecode Alliance's flagship runtime. Active
  security review, predictable disclosure flow, mature CVE response.
- Component Model is first-class — `springtale-wit`'s WIT world
  loads natively.
- Built in Rust. No FFI to a non-Rust JIT.
- Tier-2 platforms (aarch64-linux, x86_64-darwin, aarch64-darwin) all
  supported.

Negative:

- ~3 MB statically linked into the daemon binary.
- Cranelift's compile time for a fresh WASM module is non-trivial
  (~50–200 ms). We mitigate with the WASM tier cache
  (`crates/springtale-connector/src/wasm/tier/cache.rs`).
- Wasmtime updates require care — the API surface evolves between
  major releases. We pin via `dependabot.yml` ignore + manual review.

## Alternatives considered

### Option A — Wasmtime (picked)

Pros and cons enumerated above.

### Option B — Wasmer

Pros: also Rust, also mature, lower compile times than Cranelift
historically.
Cons: less aggressive sandbox surface (no epoch-based deadlines as
of when we surveyed; check current state). Smaller maintenance team.
Their commercial focus has historically been hosting rather than
embedding; concerns about API stability.

Why we didn't pick it: the epoch interruption + Component Model
maturity tipped us toward Wasmtime. The compile-time gap is real but
addressed by caching.

### Option C — V8 or QuickJS via embedded JS

Pros: enormous ecosystem of JS libraries, easy connector authoring
for non-Rust devs.
Cons: JIT we don't control. JS in 2026 still has the entire
prototype-pollution / supply-chain story. WASM-of-JS via jco
componentize adds 3-5 MB per connector for the JS engine.

Why we didn't pick it: trust model is wrong. We'd be re-inviting the
entire OpenClaw problem (npm typosquatting → arbitrary code in a
"trusted" automation).

### Option D — Lua via mlua

Pros: tiny runtime, simple sandbox API, well-understood security
posture.
Cons: not Component Model. Doesn't compose with `springtale-wit`.
Limited concurrency model. Smaller community than WASM.

Why we didn't pick it: the WASM Component Model is the long-term
target. Lua is a different ecosystem.

### Option E — No sandbox; trust signed manifests

Pros: simplest. Native connectors only.
Cons: makes community connectors impossible. The whole "OpenClaw
without the CVEs" pitch depends on a real sandbox.

Why we didn't pick it: defeats half the threat model.

## References

- `crates/springtale-connector/src/wasm/` — sandbox implementation
- `crates/springtale-connector/src/wasm/tier/cache.rs` — module compile cache
- [Wasmtime security model](https://docs.wasmtime.dev/security.html)
- [Wasmtime epoch interruption docs](https://docs.wasmtime.dev/api/wasmtime/struct.Engine.html#method.increment_epoch)
- Related: ADR 0003 (rustls only), since both choices flow from the
  same supply-chain caution
