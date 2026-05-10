# `docs/arch/` — As-Built Architecture

This folder describes **what Springtale actually is right now**, derived
from reading the working tree on 2026-04-10.

## Why this exists

`docs/current-arch/` is the locked design reference — audited, signed
off, and not to be edited. It describes intent and the full threat-model
philosophy. `docs/intended-arch/` contains forward-looking specs
(COOPERATION.md, SECURITY.md drafts).

Neither tracks the code as it drifts. This folder does.

```
docs/
├── current-arch/    locked intent (do not edit)
├── intended-arch/   forward spec drafts
└── arch/            ← reality, updated as code changes
```

*Fig. 1. The three arch folders.*

When this folder and `current-arch/` disagree, **this folder is reality
and `current-arch/` is intent**. Both are useful; they answer different
questions.

```
   Question                           Read this
   ──────────────────────────────────  ────────────────────────
   "How does it work today?"           docs/arch/
   "How SHOULD it work?"               docs/current-arch/
   "What's the plan for X feature?"    docs/intended-arch/
   "What are the known gaps?"          docs/arch/AUDIT-NOTES.md
   "Is feature Y implemented?"         docs/ROADMAP.md State col
```

*Fig. 2. Routing questions to the right doc.*

## Contents

| File | Answers |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | What are the crates, how do they wire together, what's the boot sequence, what's the API surface, what's the data flow |
| [`SECURITY.md`](SECURITY.md) | What crypto primitives ship, where are secrets handled, what does the WASM sandbox actually enforce, how does API auth work |
| [`AUDIT-NOTES.md`](AUDIT-NOTES.md) | Known drift — in-memory job queue, formation→rules generation, blackboard log unbounded, etc. |

## When to update

Update these docs whenever reality moves:

- New crate or major module → add to `ARCHITECTURE.md` tree + dep graph
- New HTTP route → add to `ARCHITECTURE.md §9`
- New security primitive or changed parameter → update `SECURITY.md`
- Fixed an `AUDIT-NOTES.md` item → delete it; add what replaced it if
  not obvious from the code

These are loose IEEE style — numbered sections, tables, ASCII figures,
file:line citations. Not academic prose. Keep them terse and verifiable.

## Who the audience is

- New contributors trying to find their way in
- Old contributors double-checking what the code actually does
- Security reviewers comparing claims in `current-arch/SECURITY.md` to
  shipping reality
- Future you, six months from now, who has forgotten where the boot
  sequence crosses crate boundaries
