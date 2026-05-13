# Privacy policy

Springtale does not collect, transmit, store, or share any data
about its users with anyone.

That's the entire policy. The rest of this page exists because
"trust us" isn't enough, and because some details are useful to
know even when the headline is "we don't have your data".

## What we don't do

- **No telemetry.** The daemon does not phone home. Not for usage
  metrics, not for crash reports, not for "anonymized" anything.
  This is enforced at the architectural level — there is no
  outbound network call from the daemon's own code path.
- **No accounts.** There is no Springtale account. No email
  required, no phone number, no real name. Your identity is an
  Ed25519 keypair generated locally on first run; it lives in
  your vault and nowhere else.
- **No analytics.** No third-party SDK, no event tracker, no fingerprinting.
- **No update check.** The daemon doesn't reach out to see if a
  new version is available. You decide when to upgrade.
- **No license check.** The software is MIT-licensed and runs without
  any contact with a licensing server. There isn't one.

## What is collected from you

Nothing, by Springtale itself.

Things you tell *connectors* are different — that's between you and
each connector's target service:

- `connector-telegram` knows your Telegram bot token and sends
  messages through Telegram's API. Telegram sees the messages
  the bot sends.
- `connector-bluesky` posts to Bluesky on your behalf. Bluesky's
  servers see those posts.
- `connector-github` calls GitHub's API. GitHub sees the calls.

Each of those services has its own privacy policy. Springtale acts
on your behalf; the upstream service treats your traffic the same
as if you'd called their API directly.

If you don't want a service to see something, don't wire a
connector for it.

## What is stored locally

All Springtale state lives on your device, in
`$SPRINGTALE_DATA_DIR` (default `~/.local/share/springtale/`):

- **Vault** (`vault.bin`) — AEAD-encrypted with your passphrase.
  Contains your Ed25519 identity, connector credentials, and
  duress region.
- **Database** (`springtale.db`) — AEAD-encrypted at rest.
  Contains rules, events, formation state, mental models, and
  audit trail.
- **API token** (`api_token`) — HMAC-SHA256 of your passphrase,
  used to authenticate CLI / dashboard / Tauri to the daemon.
- **Logs** — operational logs (rule names, connector names,
  outcomes, timing). Goes wherever your run-mode directs (journald,
  stdout, files).

You control all of it. You can `rm -rf` the data dir at any time
to factory-reset.

## What the audit trail contains

The `audit_trail` SQLite table records every action the daemon
dispatched: timestamp, connector name, action name, verdict,
outcome. It does NOT record:

- The payload content of the action (no chat message bodies, no
  file contents, no AI prompts).
- Secrets (`Secret<T>` fields are excluded).
- User-identifying details beyond what was already on the action
  metadata.

Retention is governed by `[sentinel] audit_retention_days` (default
90, configurable to 0 for "keep forever" — though "keep forever"
is rarely a good idea for forensic-trail data).

## What logs contain

Structured JSON to stdout. Operational events: rule fires, dispatch
outcomes, sentinel verdicts. No payload content. No secrets.

If you find anything secret-looking in your logs, that's a bug —
report through [`SECURITY.md`](../SECURITY.md).

## What memory contains

Two things called "memory":

- **Bot session memory** (`bot_memory` table). For chat bots, this
  is conversation history with users — bounded by `[bot]
  context_window`, AEAD-encrypted at rest. The bot uses it for
  conversational context.
- **Cooperation mental model** (`mental_model_*` tables). What a
  formation has *learned* — domain knowledge, successful
  patterns, vocabulary. Doesn't contain user content directly; can
  retain inference about user content. Can be turned off per-formation.

Both are local. Neither is transmitted anywhere.

## Cookies, IP addresses, fingerprinting

None. There is no Springtale-operated server. The daemon runs on
your device.

If you serve the daemon's HTTP API behind a reverse proxy with
its own logging, that proxy sees IP addresses of clients connecting
to it. That's between you and your proxy. Springtale itself doesn't
log IP addresses beyond what `tracing` records for opened
connections in `debug` log level.

## Sharing

There is no sharing. There is no one to share with. There is no
upstream we transmit to.

We have not received any subpoenas, gag orders, NSLs, or other
legal compulsion to share data, because we have no data to share.
The warrant canary, such as it is, lives implicitly in the fact
that you can audit the daemon's network behaviour yourself
(`tcpdump`; you see nothing at idle).

## Cross-border / GDPR / CCPA

We don't process personal data, so most data-protection regimes
don't apply to us. They may apply to **you** if you process other
people's data through Springtale (e.g. running a moderation bot
that handles user messages). That's your responsibility; Springtale
doesn't help or hinder.

## Changes to this policy

Material changes to "we collect nothing" would be a major version
bump and a CHANGELOG entry under **Security**. If telemetry were
ever proposed (which it won't be — see the architectural
constraints in [ADR 0010](adr/0010-default-deny-approval-gate.md)
and the founding rules in [CLAUDE.md](../CLAUDE.md)), it would
require a PR that the maintainer would reject.

## Contact

Privacy questions: open a Discussion. We'll answer publicly so the
answer is auditable.

Anything that smells like a privacy *vulnerability* — a path
through which the daemon leaks data we said it wouldn't —
disclose through [`SECURITY.md`](../SECURITY.md).
