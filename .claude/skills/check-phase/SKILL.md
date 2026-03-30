---
name: check-phase
description: Verify current implementation stays within the target phase scope
allowed-tools: Read, Grep, Glob, Bash
---

Check that the workspace stays within the declared build phase.

1. Read `docs/current-arch/ARCHITECTURE.md` Phase Roadmap (§3) to understand phase boundaries
2. Check for Phase 2+ implementations that shouldn't exist yet:
   - Search for AI adapter implementations beyond `NoopAdapter` (Phase 2a)
   - Search for HTTP transport implementations (Phase 2a)
   - Search for Veilid/rekindle imports (Phase 3)
   - Search for Tauri dependencies (Phase 2b)
   - Search for chat connector implementations beyond Telegram (Phase 2a)
3. Verify stubs exist but aren't implemented:
   - `VeilidTransport` should have `unimplemented!()` bodies
   - Phase 2 connectors should be commented out in workspace members
4. Report any scope violations with file paths and line numbers
