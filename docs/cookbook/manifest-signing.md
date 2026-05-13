# Recipe: Manifest signing

Springtale's WASM connectors are loaded against signed manifests.
If you build a community connector, you sign its manifest with an
Ed25519 key; Springtale verifies the signature before loading. If
you install someone else's connector, your daemon needs to know whose
keys to trust.

This recipe covers: generating a signing key, signing a manifest,
publishing your public key, and adding someone else's key to the
trusted-author list.

## Generating a signing key

```bash
springtale-cli crypto generate-author-key \
  --name "my-author-name" \
  --output ~/.springtale/keys/my-author.toml
```

Output is a TOML file with the keypair:

```toml
# my-author.toml
[author]
name = "my-author-name"
public_key = "ed25519:<base64-pubkey>"
private_key = "ed25519:<base64-privkey>"     # KEEP THIS SECRET
created_at = "2026-05-11T00:00:00Z"
```

The private key is what you sign with. Treat it the same as the
vault passphrase — anyone with it can sign manifests in your name.

You can put the keypair in the vault for daemon-side signing:

```bash
springtale-cli vault import-author-key --from ~/.springtale/keys/my-author.toml
```

Or keep it offline and only bring it out for signing operations.

## Signing a manifest

A connector manifest looks like:

```toml
# connector-mything.toml
[connector]
name = "connector-mything"
version = "0.1.0"
description = "An example community connector"

[author]
name = "my-author-name"
public_key = "ed25519:<base64-pubkey>"     # MUST match the signing key

[capabilities]
network_outbound = ["api.mything.com"]

[[triggers]]
name = "thing_happened"
schema = "..."

[[actions]]
name = "do_thing"
schema = "..."

[wasm]
url = "https://example.com/connector-mything.wasm"
sha256 = "<hex hash>"

[signature]
# Filled in by the sign command.
```

Sign it:

```bash
springtale-cli crypto sign-manifest \
  --manifest connector-mything.toml \
  --key ~/.springtale/keys/my-author.toml
```

The command computes:

```
signature_bytes = Ed25519_sign(
  private_key,
  blake3(canonical_serialization(manifest_without_signature_section))
)
```

And writes back into the manifest:

```toml
[signature]
algorithm = "ed25519"
value = "<base64-signature-bytes>"
signed_at = "2026-05-11T00:00:00Z"
```

The signature covers everything except the `[signature]` section
itself. Re-signing produces a new signature for the same content
(timestamp is included in the signed data).

## Verifying a manifest locally

```bash
springtale-cli crypto verify-manifest --manifest connector-mything.toml
```

Reports success or which check failed (signature mismatch, hash
mismatch, missing author).

The daemon does the same verification at install time. A manifest
that fails verification refuses to load with `E009`.

## Publishing your public key

For other people to verify your manifests, they need your public key.
Three ways to publish:

### Inline in the manifest

Already shown — the `[author] public_key` field is in every manifest
you sign. This is "self-attesting" — the manifest claims who signed
it. Someone who already trusts you can verify by comparing the
public_key to your known one.

### Trusted authors file

Springtale loads trusted public keys from
`$SPRINGTALE_DATA_DIR/trusted_authors.toml`:

```toml
[[author]]
name = "my-author-name"
public_key = "ed25519:<base64-pubkey>"
trust_level = "explicit"     # one of: explicit, community, trusted-on-first-use

[[author]]
name = "ScopeCreep-zip"
public_key = "ed25519:<their-pubkey>"
trust_level = "explicit"
```

Add an author:

```bash
springtale-cli authors add \
  --name "ScopeCreep-zip" \
  --public-key "ed25519:<their-pubkey>" \
  --trust-level explicit
```

`trust_level`:

- **`explicit`** — you added them, you trust them. Default for
  manually-added entries.
- **`community`** — pulled from a community trusted-keys list (not
  yet implemented; planned for Phase 3 / Veilid).
- **`trusted-on-first-use`** — first time you saw this key, you
  accepted it. Daemon will refuse to load a manifest from this
  author if the key has changed since (key rotation requires
  re-acceptance).

### Out-of-band (PGP, web of trust, etc.)

Springtale doesn't have an opinion. If you've verified your friend's
public key over Signal or in person, that's good enough. Just add
them to `trusted_authors.toml`.

## Installing a signed manifest

```bash
springtale-cli connector install --manifest ./connector-mything.toml
```

The daemon:

1. Parses the manifest.
2. Verifies the signature with the public_key field.
3. Checks the public_key is in `trusted_authors.toml` (unless
   `--allow-unknown-author` is passed; see below).
4. Downloads the WASM blob and verifies its sha256 hash matches
   the manifest.
5. Verifies the capability declarations are satisfiable (no toxic
   pairs per sentinel).
6. Registers the connector.

Failure at any step aborts. The connector doesn't load.

## Installing without trusting the author

For ad-hoc testing:

```bash
springtale-cli connector install --manifest ./connector-mything.toml \
  --allow-unknown-author
```

This bypasses the trusted-authors check. **The signature is still
verified** (signature mismatch fails install regardless). What this
disables is the "do I trust this author" gate.

Useful for trying new community connectors before you commit to
trusting the author. Don't leave it on for production loads.

## Key rotation

If your signing key is compromised, you need to:

1. Generate a new keypair.
2. Re-sign every manifest you'd previously published.
3. Publish your new public key.
4. Revoke the old public key — convention is to publish a "revocation
   record" signed by the new key declaring the old one
   compromised. Springtale doesn't have a formal revocation
   protocol yet; that's a Phase 3 / DHT concern.

In the meantime:

```bash
springtale-cli authors remove --public-key "ed25519:<old-pubkey>"
springtale-cli authors add --name "myname" --public-key "ed25519:<new-pubkey>"
```

Tell your downstream users out-of-band.

## Re-verification on every load

The daemon re-verifies signatures every time it loads a connector,
not just at install. So even if someone tampers with the manifest
file on disk between installs, the daemon catches it at next boot.

See [`docs/arch/AUDIT-NOTES.md §5`](../arch/AUDIT-NOTES.md) for the
current status of re-verification (a known item we're closing).

## Gotchas

- **The signature covers the canonical serialization.** Reformat
  the TOML (different key order, different whitespace) and the
  signature still verifies — we canonicalize before hashing.
  *But* if you change any value (even a comment near the signed
  data), verification fails.
- **The `sha256` field in `[wasm]` is part of the signed content.**
  If you re-upload the WASM blob to a new URL or recompile, the
  signature is invalid; re-sign.
- **First-party connectors don't currently ship signed manifests.**
  They're native Rust, loaded in-process, trusted by code review +
  the workspace's CI verification. Manifest signing is for WASM
  community connectors. See [ADR 0002](../adr/0002-wasmtime-not-wasmer.md).
- **Toxic-pair check happens at install + every load.** A manifest
  with `KeychainRead` + `NetworkOutbound` is rejected — see
  [`docs/arch/SECURITY.md §10.3`](../arch/SECURITY.md).
