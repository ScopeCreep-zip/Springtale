# Architecture Changelog
> Changes from `intended-arch` → `current-arch`
> Audited: 2026-03-28

This document tracks all changes made to the architecture during the audit.
The `current-arch` docs supersede `intended-arch` where they differ.

---

## Summary of Changes

### New Sections Added
1. **§2.5 Vulnerable User Threat Model** — threat modeling for trans, POC, activists, IPV survivors
2. **§2.6 Duress & Plausible Deniability** — panic wipe, duress passphrase, travel mode
3. **§2.7 Device Seizure Protection** — border crossing, law enforcement, physical access scenarios
4. **§2.8 Intimate Partner Violence (IPV) Threat Model** — stalkerware, shared device, physical access
5. **§2.9 Social Graph Protection** — metadata minimization, activity pattern protection
6. **§16 Accessibility & Inclusion** — screen readers, i18n, low-bandwidth, older devices

### Updated Sections
1. **§1 Mission & Philosophy** — added explicit statement about target community
2. **§2.1 Threat Model** — expanded with vulnerable user scenarios
3. **§3 Phase Roadmap** — duress features added to Phase 2b, travel mode to Phase 2b
4. **§5 Cargo Workspace Dependencies** — wasmtime pin updated >=41.0.4 → >=42.0.0
5. **§13 Ecosystem & Prior Art** — NosytLabs analysis updated with current findings

### Updated in SECURITY.md
1. **§1.1 OpenClaw Architecture Summary** — updated to Python/FastAPI (was Node.js), added default binding risk, updated ClawHub stats
2. **§1.5 NosytLabs Competitive Analysis** — new section with current product catalog, framework-vs-ad-hoc comparison, MCP supply chain context (CVE-2025-6514, Postmark breach)
3. **§9.2 Dependency Supply Chain** — wasmtime entry updated >=41.0.4 → >=42.0.0 to cover CVE-2026-27204 (resource exhaustion)

### Updated in ARCHITECTURE.md
1. **Phase 2a §3** — updated OpenClaw description: 250K+ stars (was 310K+), Python/FastAPI (was Node.js), added ClawHavoc and CVE context
2. **§13.7 NosytLabs** — new subsection with current analysis, repos removed, framework-vs-ad-hoc framing
3. **Phase 2b** — added safety features list (duress, panic wipe, travel mode, quick-hide, app disguise)

### No Changes (Validated as Sound)
- Transport abstraction (§6.3, §11) — correct and clean
- Connector sandbox model (§6.4) — genuinely better than all competitors
- NoopAdapter philosophy (§6.7) — correct for longevity
- Secret<T> type system (§2.4) — compile-time prevention works
- Recursive pipeline (§14.3) — well-designed fuel metering
- Phase discipline (§3) — clear boundaries, stubs appropriate
- Rekindle integration plan (§7 Phase 3) — architecturally sound
- All OWASP ASVS mappings (SECURITY.md §3) — thorough and correct
- Privacy by Design mappings (SECURITY.md §8) — comprehensive
- CI Security Pipeline (SECURITY.md §9.1) — complete and appropriate
- Rekindle architecture (rekindle-architecture.md) — no changes needed
