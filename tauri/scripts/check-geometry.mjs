#!/usr/bin/env node
// Plan 3.5 — the geometry check.
//
// `packages/ui` ships no test runner, and `getAgentPosition` is the one piece
// of canvas logic where a wrong answer is invisible in a screenshot but wrong
// in every frame. So it gets a real check rather than a new toolchain: the
// TypeScript compiler (already a devDependency) transpiles the two pure
// modules into a temp directory, and this script imports and exercises the
// real function — not a copy of it.
//
// Cases, from the plan: a firing agent lands between its tree and its action
// target; an idle agent lands at home; two unconnected agents do not overlap.
//
// Usage: node scripts/check-geometry.mjs   (exit 1 on any failure)

import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "packages/ui/src/colony");

const out = mkdtempSync(join(tmpdir(), "springtale-geometry-"));
function emit(name) {
  const js = ts.transpileModule(readFileSync(join(SRC, `${name}.ts`), "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText;
  writeFileSync(join(out, `${name}.mjs`), js.replaceAll('from "./types"', 'from "./types.mjs"'));
}

const failures = [];
function check(name, ok, detail) {
  if (!ok) failures.push(`${name}${detail ? `: ${detail}` : ""}`);
}

try {
  emit("types");
  emit("geometry");
  const { getAgentPosition } = await import(pathToFileURL(join(out, "geometry.mjs")).href);

  const nodes = [
    { id: "tree-a", label: "tree-a", type: "conifer", x: 20, y: 20, status: "active" },
    { id: "tree-b", label: "tree-b", type: "conifer", x: 80, y: 60, status: "active" },
  ];
  const agent = (over) => ({
    id: "agent-1",
    name: "a",
    role: "worker",
    autonomy: 1,
    autonomyLabel: "SUGGEST",
    fuel: 100,
    fuelStatus: "ok",
    hp: 100,
    connectorId: "tree-a",
    actionConnectorId: null,
    task: "",
    status: "ok",
    pipeline: null,
    attentionLoad: 0,
    liveness: 1,
    healthState: "healthy",
    ...over,
  });

  // 1. A firing agent walks part-way toward its action target.
  const firing = getAgentPosition(
    agent({ actionConnectorId: "tree-b", activity: "firing" }),
    nodes,
    {},
  );
  check(
    "firing agent is between its tree and its action target",
    firing.x > 20 && firing.x < 80 && firing.y > 20 && firing.y < 60,
    `got ${JSON.stringify(firing)}`,
  );

  // …and the same agent, not firing, stays home.
  const idle = getAgentPosition(
    agent({ actionConnectorId: "tree-b", activity: "idle" }),
    nodes,
    {},
  );
  check(
    "an idle agent stands at its own tree",
    Math.abs(idle.x - 20) <= 6 && idle.y > 20 && idle.y - 20 <= 16,
    `got ${JSON.stringify(idle)}`,
  );

  // 2. Firing with no action target is still home — no invented movement.
  const noTarget = getAgentPosition(agent({ activity: "firing" }), nodes, {});
  check(
    "a firing agent with no RunConnector target stays home",
    Math.abs(noTarget.x - idle.x) < 1e-9 && Math.abs(noTarget.y - idle.y) < 1e-9,
    `got ${JSON.stringify(noTarget)}`,
  );

  // 3. Unconnected agents spread instead of stacking.
  const loose = ["loose-one", "loose-two", "loose-three"].map((id) =>
    getAgentPosition(agent({ id, connectorId: null }), nodes, {}),
  );
  const seen = new Set(loose.map((p) => `${p.x},${p.y}`));
  check(
    "unconnected agents do not overlap",
    seen.size === loose.length,
    `positions ${JSON.stringify(loose)}`,
  );

  // 4. Standoff grows with attention load — a busy agent stands further out.
  const calm = getAgentPosition(agent({ attentionLoad: 0 }), nodes, {});
  const busy = getAgentPosition(agent({ attentionLoad: 1 }), nodes, {});
  check("standoff grows with attention load", busy.y > calm.y, `${calm.y} → ${busy.y}`);
} finally {
  rmSync(out, { recursive: true, force: true });
}

if (failures.length > 0) {
  process.stderr.write(`check-geometry: ${failures.length} failure(s)\n`);
  for (const f of failures) process.stderr.write(`  ${f}\n`);
  process.exit(1);
}
process.stdout.write("check-geometry: ok\n");
