#!/usr/bin/env node
/**
 * Plan 3.3 — the command card does what it says.
 *
 * `packages/ui` has no test runner, so this is the cheapest real check that
 * every command-grid button in every context reaches exactly one handler:
 *
 *  1. Every verb in a grid — the static `COMMANDS` table plus the canonical
 *     formation grid the Rust backend serves — has a `case` in the one
 *     `handleCommand` switch. A button with no case is an inert button.
 *  2. Every `case` in that switch is reachable: it is a grid verb, or some
 *     component dispatches it by name. A case with no dispatcher is a dead
 *     verb (this is how `agent:recall` and `formation:add` survived).
 *
 * Run by `pnpm lint`.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const tauriRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(tauriRoot, "..");
const read = (p) => readFileSync(p, "utf8");

const typesPath = join(tauriRoot, "packages/ui/src/colony/types.ts");
const controllerPath = join(tauriRoot, "packages/ui/src/colony/controller.ts");
const rustPath = join(repoRoot, "crates/springtale-runtime/src/operations/commands.rs");

/** Verbs in the frontend-owned 3x3 grids. */
const gridBlock =
  read(typesPath).split("export const COMMANDS")[1]?.split("export const NODE_TYPES")[0] ?? "";
const gridVerbs = new Set([...gridBlock.matchAll(/"([a-z_]+:[a-z_]+)"\)/g)].map((m) => m[1]));

/** Verbs in the backend-owned formation grid (Rust is the source of truth). */
const rustBlock = read(rustPath).split("FORMATION_COMMANDS_CANONICAL")[1]?.split("];")[0] ?? "";
const formationVerbs = new Set([...rustBlock.matchAll(/"(formation:[a-z_]+)"/g)].map((m) => m[1]));

/** Verbs the one switch handles. */
const controller = read(controllerPath);
const cases = new Set([...controller.matchAll(/^\s*case "([a-z_]+:[a-z_]+)":/gm)].map((m) => m[1]));

/** Every verb literal dispatched anywhere in the frontend. */
const sources = ["packages/ui/src", "apps/desktop/src", "apps/dashboard/src"];
const dispatched = new Set();
const walk = (dir) => {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) {
      walk(p);
      continue;
    }
    if (!/\.(ts|tsx)$/.test(entry)) continue;
    for (const line of read(p).split("\n")) {
      if (p === controllerPath && /^\s*case "/.test(line)) continue;
      for (const m of line.matchAll(/"([a-z_]+:[a-z_]+)"/g)) dispatched.add(m[1]);
    }
  }
};
for (const s of sources) walk(join(tauriRoot, s));

const errors = [];
for (const verb of [...gridVerbs, ...formationVerbs].sort()) {
  if (!cases.has(verb)) {
    errors.push(`grid button "${verb}" has no case in handleCommand (inert button)`);
  }
}
for (const verb of [...cases].sort()) {
  if (!gridVerbs.has(verb) && !formationVerbs.has(verb) && !dispatched.has(verb)) {
    errors.push(`handleCommand case "${verb}" is never dispatched (dead verb)`);
  }
}

if (errors.length > 0) {
  process.stderr.write(`check-command-verbs: ${errors.length} problem(s)\n`);
  for (const e of errors) process.stderr.write(`  - ${e}\n`);
  process.exit(1);
}
process.stdout.write(
  `check-command-verbs: ok (${gridVerbs.size} static + ${formationVerbs.size} formation verbs)\n`,
);
