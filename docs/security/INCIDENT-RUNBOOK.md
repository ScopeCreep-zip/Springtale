# Incident Response Runbook (Maintainer-facing)

Step-by-step playbook for supply-chain compromise, leaked credentials,
unauthorized releases, and reported user-safety vulnerabilities. Per [NIST CSF
2.0 Respond function][csf2] and [CISA CVD Program][cisa-cvd].

Last review: 2026-05-13.

[csf2]: https://nvlpubs.nist.gov/nistpubs/CSWP/NIST.CSWP.29.pdf
[cisa-cvd]: https://www.cisa.gov/resources-tools/programs/coordinated-vulnerability-disclosure-program

## Severity guidance

| Sev | Examples | First action SLA |
|---|---|---|
| **Critical** | Vault key extraction; panic wipe defeat; identity leak; telemetry phone-home; remote unauthenticated RCE | ≤4h |
| **High** | Connector sandbox escape; capability bypass; auth bypass on management API | ≤24h |
| **Medium** | Path traversal in a single connector; logged secret; CSP bypass | ≤72h |
| **Low** | Verbose error leaking software version; ICU/font load over plaintext on dashboard | ≤7d |

User-safety bugs (identity leak across contexts, duress detectability, panic
wipe defeat, any phone-home) are **always Critical** regardless of CVSS.

## Playbook 1 — Compromised upstream dependency

Trigger: RustSec / npm-advisory / KEV match on a dep we ship; or any Dependabot
security PR landing in our queue.

1. **Triage** — Is this dep on our release path?
   - Yes: stop, follow steps 2–7.
   - No: open a `cargo deny` issue to ban + advance to step 8.
2. **Pin / rollback** — Open a feature branch reverting to the last
   known-good version. If patched upstream exists, bump there instead.
3. **Confirm green** — Run full CI (`gh workflow run ci.yml`, `sca.yml`,
   `sbom.yml`) on the branch.
4. **Audit usage** — `cargo tree -i <crate>` to enumerate everything that
   pulls it. Read each call site. Document in the PR body.
5. **Issue a VEX** — Add `vex/<crate>-<cve>.json` declaring affected/
   not_affected with justification. OpenVEX format.
6. **Merge + tag patch release** — `v0.X.Y-secfix.1`. Run `provenance.yml`
   to sign + attest.
7. **Publish advisory** — GitHub Security Advisory with CWE-ID, CVSS, and
   patched version. Update `CHANGELOG.md` security section.
8. **Long-term** — Add the crate to `dependabot.yml` `ignore` list if it
   needs manual review going forward. Update `docs/security/SUPPLY-CHAIN.md`
   if policy changed.

## Playbook 2 — Leaked publish credential (crates.io / npm / PyPI / cosign)

Trigger: maintainer detects unauthorized publish, or GitHub secret scanning
fires, or upstream registry sends a compromised-account email.

1. **Revoke immediately**:
   - crates.io API token → Settings → Account → revoke
   - npm classic token → npmjs.com/settings/<user>/tokens → revoke
   - PyPI token → PyPI account → revoke
   - GitHub PAT → Settings → Developer settings → revoke
   - Sigstore: OIDC tokens are short-lived; no revoke needed, but rotate
     the linked GitHub identity if compromised
2. **Yank malicious releases** — `cargo yank <ver>`, `npm unpublish` (within
   window), `pip-yank`, or contact registry support.
3. **Audit publish history** — Identify any unauthorized version. Compare
   SHA-256 against signed-bundle expectations.
4. **Rotate signing identity** — Re-do Sigstore keyless flow with fresh
   GitHub OIDC; cosign verify recipe in `docs/operations/verifying-releases.md`
   carries new identity claim.
5. **Notify** — GitHub Security Advisory; CHANGELOG security section;
   mailing list / channel where users learned of releases.
6. **Replay attestation** — Build the legitimate version on a clean runner;
   sign + push provenance; instruct users to verify against new identity.
7. **Move to OIDC trusted publishing** if not already on it.

## Playbook 3 — Unauthorized release tag / Action compromise

Trigger: a release tag appears on `main` that wasn't created by the maintainer,
or a GitHub Action we use is reported compromised (e.g. tj-actions Mar 2025).

1. **Disable the workflow** that consumed the compromised action — revert
   on `main` immediately. Branch-protect to require all workflows reviewed.
2. **Audit Actions logs** — search for the compromise window's runs;
   download logs; look for known exfil indicators (double-base64 strings,
   exfil POSTs to non-allowed endpoints).
3. **Rotate every token used** in the window. See Playbook 2.
4. **Yank or unpublish** any artifact built by a compromised workflow.
5. **Update `docs/security/CI-TRUST.md`** — remove the action from the
   allowlist or pin to known-safe SHA.
6. **Update org Action policy** to block the compromised action.
7. **Notify** — Security Advisory.
8. **Re-publish** clean artifacts from the new SHA-pinned workflow with
   full attestation.

## Playbook 4 — Reported user-safety vulnerability

Trigger: GitHub Security Advisory opened or email to maintainer with anything
in the user-safety class (identity leak, duress detectability, panic wipe
defeat, telemetry).

1. **Acknowledge ≤24h** to the reporter.
2. **Verify** — Try to reproduce on the current main against a fresh vault.
3. **Triage** — Critical user-safety always. CVSS often understates impact;
   we score by reach into the threat model, not raw CVSS.
4. **Develop fix on private branch** — ≤14d target. Coordinate with reporter
   if they want to be credited.
5. **Test** — Full CI + manual reproduction. Add regression test.
6. **Coordinated disclosure** — Negotiate window with reporter (default 90d,
   shorter for active exploitation).
7. **Publish** — Security Advisory, signed patch release, CHANGELOG entry,
   credit reporter (real name, handle, or anonymous per their preference).
8. **Post-mortem** — Document root cause; update STRIDE register
   (`RISK-REGISTER.md`); add lint or property test that would have caught it.

## Playbook 5 — KEV daily-check fires

Trigger: `.github/workflows/sca.yml` `kev-check` job fails.

1. **Read the job output** — which dep matches which KEV entry.
2. **Confirm not a false positive** — sometimes KEV matches by CPE but the
   advisory doesn't apply to our usage path.
3. If real: follow Playbook 1 from step 2.
4. If false positive: add a VEX `not_affected` justification in `vex/`,
   re-run.

## Playbook 6 — Prompt injection / AI guardrail bypass

Trigger: `llm-redteam.yml` corpus run finds a bypass, or a user reports
exfiltration via an AI adapter.

1. **Identify the adapter** and the prompt path.
2. **Quarantine** — disable the adapter in `springtale.toml.example`
   defaults; document in `CHANGELOG.md`.
3. **Patch `AiGuardrail`** — strengthen redaction / output validation /
   capability check.
4. **Add the attack to the corpus** — `tests/llm-redteam/corpus/`.
5. **Verify with `llm-redteam.yml`** — must refuse.
6. **Release with advisory** if user data exposure occurred.

## Communication templates

### Security Advisory body

```
**Summary**: <one-line>
**Affected**: <versions>
**Patched**: <version>
**CVE**: <if-assigned>
**CWE**: <id>
**CVSS v4**: <vector + score>
**User-safety impact**: <how this affects vulnerable users specifically>
**Reporter**: <credit>
**Acknowledgement**: <date>
**Triage**: <date>
**Fix**: <date>
**Disclosure**: <date>
```

### CHANGELOG.md security section line

```
- security: <scope> — fix <CWE/CVE>; user-safety impact: <one line>. Credit: <reporter>.
```

## Roles

For now (single maintainer):

- **Triage / Triage owner**: `@radicalkjax`
- **Crypto reviewer**: `@radicalkjax`
- **Connector reviewer**: `@radicalkjax`
- **Release engineer**: `@radicalkjax`

When the project grows, this section gets per-role assignees. The
runbook itself does not change.

## On becoming a CNA

### Current path — GitHub Advisories CNA proxy

GitHub Advisories acts as our CNA proxy today. The flow:

1. A vulnerability is reported via this repo's private vulnerability
   reporting surface (`SECURITY.md` → "Reporting a Vulnerability").
2. The maintainer files a GitHub Security Advisory in the
   `Security` → `Security advisories` tab.
3. From the advisory, click `Request CVE` — GitHub assigns a CVE
   ID under their CNA scope (CVE-2026-XXXXX namespace).
4. The advisory is published once the fix lands in `main`, with the
   CVE ID embedded.
5. Disclosure timeline follows the policy in `SECURITY.md` (typically
   90 days from initial report, sooner if exploitation is observed).

This path requires zero CNA registration on Springtale's part. It is
the recommended posture per [GitHub's CNA documentation][gh-cna] and
[CVE Numbering Authority program rules][cna-rules] for projects under
the GitHub-as-CNA umbrella.

### Future option — Springtale as a direct CVE Numbering Authority

**Why we might want this:** a direct CNA registration gives Springtale
control over its own CVE namespace, sets disclosure timelines without
GitHub's gatekeeping, and makes the project visible in the [CNA
list][cna-list]. The signal value for vulnerable users — "this project
is mature enough to own its security identifier issuance" — is real.

**Trigger to apply:**

- ≥1 CVE assigned per quarter sustained for 4 consecutive quarters
  (this is roughly MITRE's published threshold for considering a
  project's CNA application credible).
- AND demonstrated 90-day-or-better disclosure track record on at
  least 4 prior assigned CVEs.
- AND a documented security contact reachable via at least two
  independent channels (the existing `security.txt` + the GitHub
  advisories surface satisfies this once both are stable).

**Application process (per [cve.org/About/CNARules][cna-rules]):**

1. Read the CNA Operational Rules — currently CNA Rules v4.0.
2. Identify the CNA scope statement: which products fall under
   Springtale's CNA. The natural scope is "vulnerabilities in
   software developed by the Springtale project, including connector
   crates published under the project's namespace." Third-party
   connectors are out of scope.
3. Identify a Root CNA. For an open-source project of this scope
   the Root CNA is [MITRE][mitre-root] (the top-level Root for
   projects without an existing Root assignment).
4. Submit the application via the CNA registration form on
   [cve.org/PartnerInformation][cna-registration]. Required fields:
   - Organization name and abbreviation (e.g. "Springtale")
   - Scope statement (see step 2)
   - Disclosure policy URL (point at `SECURITY.md`)
   - Security contact (point at the address in `security.txt`)
   - Web disclosure venue (this repo's Security tab — GitHub
     Advisories surface remains the publication channel)
5. Expect a ~3-month review per MITRE published timelines. The
   review consists of (a) scope sanity-check, (b) disclosure-policy
   review, (c) Root CNA approval, (d) onboarding training.
6. On acceptance, Springtale receives a CVE ID block and onboarding
   credentials for the [CVE Services API][cve-services] (Rest-based
   CVE ID reservation + publication endpoint).

**Post-acceptance commitments (the things we sign up for):**

- Timely CVE assignment — within 72 hours of a confirmed
  vulnerability report, per CNA rules.
- Annual CNA-rules compliance attestation.
- Public CNA scope statement maintained on `SECURITY.md`.
- Participation in CVE Services API + Advisory format compliance.
- Subscribe to the cve-cna-list announce channel for rule changes.

[gh-cna]: https://docs.github.com/en/code-security/security-advisories/working-with-global-security-advisories-from-the-github-advisory-database/about-the-github-advisory-database#cve-identification-numbers
[cna-rules]: https://www.cve.org/ResourcesSupport/AllResources/CNARules
[cna-list]: https://www.cve.org/PartnerInformation/ListofPartners
[mitre-root]: https://www.cve.org/PartnerInformation/Partner/MITRE
[cna-registration]: https://www.cve.org/PartnerInformation/Partner-Lookup
[cve-services]: https://github.com/CVEProject/cve-services
