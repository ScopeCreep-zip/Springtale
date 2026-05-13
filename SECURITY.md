# Security Policy

If you find a vulnerability in Springtale, this is how you tell us
about it without putting users at risk in the meantime.

This file is the **disclosure policy**. The threat model lives in
[`docs/arch/SECURITY.md`](docs/arch/SECURITY.md) (as-built) and
[`docs/current-arch/SECURITY.md`](docs/current-arch/SECURITY.md)
(design intent, including the vulnerable-user threat models in §2.5–§2.9).

## Reporting a vulnerability

**Don't open a public issue.** Use one of these channels:

1. **GitHub Security Advisories** (preferred) — open a private
   advisory at <https://github.com/ScopeCreep-zip/Springtale/security/advisories/new>.
   Only project maintainers can read it. Comes with built-in CVE
   request flow.
2. **Email** — `k1104jackson@hotmail.com`. Subject line: `SECURITY: <one-line summary>`.

If you can encrypt to a PGP key, use the maintainer key listed on the
GitHub profile. If you can't, send unencrypted — we'd rather hear from
you in plaintext than not at all.

### What to include

- A clear description of the issue.
- The affected component (crate name, file path, function).
- The version / commit hash you reproduced against.
- Reproduction steps, even if rough. A working PoC is ideal.
- Your assessment of impact: confidentiality / integrity / availability,
  and what an attacker can do with this.
- Any mitigation you've already worked out.

### What you can expect from us

| Stage | Target time |
|---|---|
| Acknowledgement that we received the report | 72 hours |
| Initial triage + severity rating | 7 days |
| Fix on a private branch (for confirmed valid reports) | 30 days |
| Coordinated public disclosure | Negotiated with the reporter |

We will:

- Tell you our triage decision and reasoning, even if we don't think
  the report is exploitable.
- Credit you in the advisory + changelog under whatever name you
  want (real name, handle, "anonymous").
- Not threaten you with legal action for reporting in good faith.

## In scope

Anything in this repository that handles user data, credentials,
secrets, or capability decisions. Concretely:

- `crates/springtale-crypto` — vault, KDF, AEAD, manifest signing
- `crates/springtale-connector` — capability system, WASM sandbox, manifest verification
- `crates/springtale-sentinel` — toxic-pair detection, audit trail, approval gate
- `crates/springtale-store` — vault file layout, encryption-at-rest, secret redaction
- `crates/springtale-transport` — TLS configuration, mTLS auth
- `crates/springtale-bot` — capability checks at dispatch, tool calling
- `apps/springtaled` — API auth, rate limiting, webhook signature verification
- `apps/springtale-cli` — passphrase acquisition, panic flow
- `tauri/apps/desktop/src-tauri/` — IPC boundary, OS-shell integration
- All first-party connectors under `connectors/`

## Out of scope (please don't report)

- Issues in dependencies that don't affect Springtale's actual usage
  path. Report those upstream and ping us if a CVE lands.
- DoS via local resource exhaustion (the daemon runs on the user's own
  machine; they can OOM themselves if they want).
- Anything that requires "the attacker has root on the user's device".
  Our model gives up once an adversary owns the kernel — see the
  threat model for what we do try to defend.
- The `VeilidTransport` stub. It's a stub. Every method returns
  `TransportError::NotConnected`. There is no attack surface yet.
- Theoretical attacks against `connector-matrix`. It's not in the
  workspace — we held it back because of an upstream `rusqlite` CVE.

## Special handling for user-safety vulnerabilities

If a bug puts a vulnerable user at concrete risk — anything that:

- Leaks identity across contexts (a pseudonym getting linked to a real
  name, a duress vault being detectable as a duress vault, panic-wipe
  leaving recoverable artefacts).
- Exposes connectors the user explicitly hid (formations leaking
  through awareness gossip, sentinel audit trail being readable
  pre-auth).
- Defeats the duress passphrase or panic wipe.
- Causes the daemon to phone home in any way, at any time, for any
  reason.

…flag it as **HIGH urgency** in your report. We will treat the response
SLA as ≤24 hours for the acknowledgement and ≤14 days for the fix,
and we will prioritise it over feature work and other security issues
of higher CVSS but lower user-impact-at-the-margin.

## After the fix

- We publish a GitHub Security Advisory with the CVE (if assigned).
- We update [`docs/arch/AUDIT-NOTES.md`](docs/arch/AUDIT-NOTES.md) and
  [`CHANGELOG.md`](CHANGELOG.md).
- We credit you (real name, handle, or anonymous — your call).
- We don't write a triumphant blog post about how cool the fix was.
  That's not what these announcements are for.

## Bounties

We don't currently have a paid bounty program — Springtale is OSS run
on personal time. If you want to be paid for finding bugs in security
software, [HackerOne](https://hackerone.com) and [Bugcrowd](https://bugcrowd.com)
have programs that do.

We do accept sponsorships (see [`.github/FUNDING.yml`](.github/FUNDING.yml))
which is the closest thing we have to a way for security researchers
to be compensated.
