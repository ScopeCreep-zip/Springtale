#!/usr/bin/env sh
# Print, one per line, every daemon route the command line calls.
#
# The command line answers this itself. `springtale dump-commands` walks
# its own clap tree at runtime and prints every verb with the routes that
# verb calls, declared beside it in `apps/springtale-cli/src/surface.rs`
# and held to the tree by a unit test — a new subcommand cannot be added
# without saying what it talks to.
#
# This used to grep path literals out of the CLI sources, which answered
# a weaker question: a path in a comment counted as a verb, and a verb
# that built its path from a constant or a `match` did not count at all.
#
# Holes are flattened to the shape the OpenAPI templates carry:
#
#   /formations/{id}/deploy   ->  /formations/{}/deploy
#
# An empty result is a FAILURE, not a clean surface, and so is a verb
# whose routes are undeclared: both mean the dump is broken.
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

# Ask cargo rather than trusting whatever is already in target/: a
# stale binary would answer for a command line that no longer exists.
# `cargo build` is a no-op when it is up to date. `SPRINGTALE_CLI`
# overrides for a packaged binary (CI, a release image).
bin="${SPRINGTALE_CLI:-}"
if [ -z "$bin" ]; then
  cargo build -q --manifest-path "$root/Cargo.toml" -p springtale-cli >&2
  bin="$root/target/debug/springtale-cli"
fi

dump="$("$bin" dump-commands)"

if ! printf '%s' "$dump" | jq -e '(.commands | length) > 0' > /dev/null; then
  printf 'cli-routes: the command tree came back EMPTY. That is a dump\n' >&2
  printf 'bug, not a command line with no verbs. Refusing to print.\n' >&2
  exit 1
fi

if ! printf '%s' "$dump" | jq -e 'all(.commands[]; .routes != null)' > /dev/null; then
  printf 'cli-routes: these verbs declare no routes at all:\n' >&2
  printf '%s' "$dump" | jq -r '.commands[] | select(.routes == null) | .verb' >&2
  printf 'Declare them in apps/springtale-cli/src/surface.rs (an offline\n' >&2
  printf 'verb declares an empty list).\n' >&2
  exit 1
fi

printf '%s' "$dump" \
  | jq -r '.commands[].routes[]' \
  | sed -e 's#/{[^}]*}#/{}#g' -e 's#/\{1,\}$##' \
  | sort -u
