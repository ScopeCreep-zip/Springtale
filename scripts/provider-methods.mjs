#!/usr/bin/env node
// Print, one per line, every daemon route the web DataProvider calls.
//
// The provider is the only place the web surface talks to springtaled,
// so its path literals ARE the provider's half of the API contract. The
// provider delegates some families to the small modules beside it
// (`web/api/*.ts`); those calls are the provider's calls, so they are
// read here too.
//
// A literal is a route with two kinds of noise stripped, the same two
// the OpenAPI templates do not carry:
//
//   `/events?limit=${limit}`             -> /events
//   `/recipes/${encodeURIComponent(id)}` -> /recipes/{}
//
// A `${hole}` is a path segment only when a `/` introduces it; a hole
// anywhere else is an interpolated query string (`${queryString(f)}`) or
// the base URL (`${getBaseUrl()}/recipes/import`), not a segment, and is
// dropped rather than turned into `{}`.

import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const web = resolve(here, "..", "tauri", "packages", "ui", "src", "web");
const providers = [
  join(web, "provider.ts"),
  ...readdirSync(join(web, "api"))
    .filter((f) => f.endsWith(".ts"))
    .map((f) => join(web, "api", f)),
];

// A `${…}` hole, allowing one level of nesting so `${queryString({ a: b })}`
// reads as one hole rather than a truncated one.
const HOLE = String.raw`\$\{(?:[^{}]|\{[^{}]*\})*\}`;
const PATH_LITERAL = new RegExp(
  String.raw`["'\`]((?:${HOLE})?\/(?:${HOLE}|[A-Za-z0-9_/.:?=&{}-])*)["'\`]`,
  "g",
);
const MARKER = "%HOLE%";

const routes = new Set();
for (const file of providers) {
  const source = readFileSync(file, "utf8");
  for (const [, path] of source.matchAll(PATH_LITERAL)) {
    const normalised = path
      .split("?")[0]
      // A hole after `/` is a path segment; any other hole is not.
      .replace(new RegExp(String.raw`\/${HOLE}`, "g"), `/${MARKER}`)
      .replace(new RegExp(HOLE, "g"), "")
      .split(MARKER)
      .join("{}")
      .replace(/\/+$/, "");
    if (normalised.length > 1 && !normalised.startsWith("//")) routes.add(normalised);
  }
}

if (routes.size === 0) {
  console.error("check-surface: provider-methods found no routes — the");
  console.error("extraction is broken, not the provider. Refusing to pass.");
  process.exit(1);
}

for (const route of [...routes].sort()) console.log(route);
