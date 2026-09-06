#!/usr/bin/env node
// Plan 3.8 (finding 64) — inline-style rule.
//
// Biome has no custom-rule engine, so this script enforces the colony
// convention from .claude/rules/frontend/solidjs-conventions.md: a JSX
// `style=` attribute may only set the four dynamic layout properties
// (`width`, `left`, `top`, `transform`) or `--colony-*` custom properties
// that a colony.css class consumes. Everything else belongs in a class
// (optionally keyed by a `data-*` attribute).
//
// Usage: node scripts/check-inline-styles.mjs   (exit 1 on any violation)
//
// TODO(plan 3.8): files listed here are still allowed to violate the rule.
// Remove an entry once its inline styles have been moved to colony CSS.
const LEGACY_FILES = new Set([]);

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SCAN_DIRS = ["apps/dashboard/src", "apps/desktop/src", "packages/ui/src"];
const ALLOWED = new Set(["width", "left", "top", "transform"]);

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "dist") continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) yield* walk(full);
    else if (full.endsWith(".tsx")) yield full;
  }
}

/** Return the balanced `{...}` expression starting at `start` (index of `{`). */
function balanced(src, start) {
  let depth = 0;
  for (let i = start; i < src.length; i++) {
    const ch = src[i];
    if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  return src.slice(start);
}

// A key is an identifier or quoted string at the start of a line or right
// after `{`/`,`, followed by `:`. Ternary branches (`? "x"\n : "y"`) do not
// match because the string is preceded by `?`, not `{`/`,`/line-start.
const KEY_RE = /(?:^|[{,])\s*(?:"(--?[\w-]+)"|'(--?[\w-]+)'|([A-Za-z_$][\w$]*))\s*:/gm;

function isAllowed(key) {
  return ALLOWED.has(key) || key.startsWith("--colony-");
}

const violations = [];
for (const dir of SCAN_DIRS) {
  for (const file of walk(join(ROOT, dir))) {
    const rel = relative(ROOT, file);
    if (LEGACY_FILES.has(rel)) continue;
    const src = readFileSync(file, "utf8");
    for (const m of src.matchAll(/\bstyle=\{/g)) {
      const expr = balanced(src, m.index + "style=".length);
      const line = src.slice(0, m.index).split("\n").length;
      const keys = [...expr.matchAll(KEY_RE)].map((k) => k[1] ?? k[2] ?? k[3]);
      if (keys.length === 0) {
        violations.push(`${rel}:${line}: opaque style expression (cannot verify keys)`);
        continue;
      }
      for (const key of keys) {
        if (!isAllowed(key))
          violations.push(`${rel}:${line}: inline "${key}" — move to colony.css`);
      }
    }
  }
}

if (violations.length > 0) {
  process.stderr.write(`check-inline-styles: ${violations.length} violation(s)\n`);
  for (const v of violations) process.stderr.write(`  ${v}\n`);
  process.exit(1);
}
process.stdout.write("check-inline-styles: ok\n");
