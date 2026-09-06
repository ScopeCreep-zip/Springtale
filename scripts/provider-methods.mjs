#!/usr/bin/env node
// Print, one per line, every daemon route the web DataProvider calls.
//
// The provider is the only place the web surface talks to springtaled,
// so its path literals ARE the provider's half of the API contract.
// Template holes (`${id}`) are normalised to `{}` so they line up with
// the OpenAPI path templates (`{id}`).

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const providers = [
  resolve(here, "..", "tauri", "packages", "ui", "src", "web", "provider.ts"),
];

const PATH_LITERAL = /["'`](\/[A-Za-z0-9_${}/.:-]*)["'`]/g;

const routes = new Set();
for (const file of providers) {
  const source = readFileSync(file, "utf8");
  for (const [, path] of source.matchAll(PATH_LITERAL)) {
    // Drop query strings and trailing slashes, normalise template holes.
    const normalised = path
      .split("?")[0]
      .replace(/\$\{[^}]*\}/g, "{}")
      .replace(/\/+$/, "");
    if (normalised.length > 1) routes.add(normalised);
  }
}

if (routes.size === 0) {
  console.error("check-surface: provider-methods found no routes — the");
  console.error("extraction is broken, not the provider. Refusing to pass.");
  process.exit(1);
}

for (const route of [...routes].sort()) console.log(route);
