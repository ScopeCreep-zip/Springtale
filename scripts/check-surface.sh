#!/usr/bin/env sh
# Surface check (plan 2.3): the daemon, the command line and the two
# front ends must describe the same API.
#
#   1. every route in the OpenAPI document has a command-line verb
#   2. every route has a web DataProvider method
#   3. nothing under /formations sits outside the four orchestration
#      verb groups (the drum rule)
#
# The document comes from a running daemon when PORT is set, and
# otherwise from the checked-in contract that `springtaled
# --dump-openapi` regenerates. Either way an empty list is a FAILURE:
# a broken extractor must never read as a clean surface.
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

contract="$root/tauri/packages/types/openapi.json"
if [ -n "${PORT:-}" ]; then
  curl -sf "http://127.0.0.1:$PORT/openapi.json" > "$work/openapi.json"
  contract="$work/openapi.json"
fi

fail=0
die() { printf 'check-surface: %s\n' "$1" >&2; fail=1; }

# ── The three lists ────────────────────────────────────────────────
jq -r '.paths | keys[]' "$contract" \
  | sed -e 's/{[^}]*}/{}/g' -e 's#/\{1,\}$##' | sort -u > "$work/routes"
sh "$root/scripts/cli-routes.sh" > "$work/verbs"
node "$root/scripts/provider-methods.mjs" > "$work/provider"

for list in routes verbs provider; do
  if [ ! -s "$work/$list" ]; then
    printf 'check-surface: the %s list is EMPTY. That is an extraction\n' "$list" >&2
    printf 'bug, not a clean surface. Refusing to pass.\n' >&2
    exit 1
  fi
done

# ── Known gaps, and deliberate non-surface ────────────────────────
# Two files, same columns, opposite meanings:
#
#   surface-exemptions.txt    a ledger of routes that do not YET have a
#                             command-line verb and/or provider method
#   surface-not-surfaced.txt  routes that deliberately never will, each
#                             with the reason on the line
#
#     /some/route        cli            # no verb yet
#     /other/route       cli provider   # neither yet
#
# Neither is a permission slip: a NEW uncovered route fails the check,
# and an entry in either file naming a route that no longer exists fails
# it too, so both can only shrink.
exempt="$root/scripts/surface-exemptions.txt"
intentional="$root/scripts/surface-not-surfaced.txt"
sed -e 's/#.*$//' -e 's/[[:space:]]*$//' "$exempt" "$intentional" \
  | grep -vE '^$' > "$work/exempt.raw"
awk '$0 ~ /(^| )cli( |$)/ { print $1 }' "$work/exempt.raw" | sort -u > "$work/exempt.cli"
awk '$0 ~ /(^| )provider( |$)/ { print $1 }' "$work/exempt.raw" | sort -u > "$work/exempt.provider"
awk '{ print $1 }' "$work/exempt.raw" | sort -u > "$work/exempt.all"

comm -23 "$work/routes" "$work/exempt.cli" > "$work/routes.cli"
comm -23 "$work/routes" "$work/exempt.provider" > "$work/routes.provider"

missing_verb="$(comm -23 "$work/routes.cli" "$work/verbs")"
[ -z "$missing_verb" ] || {
  die "routes with no command-line verb:"
  printf '%s\n' "$missing_verb" >&2
}

missing_provider="$(comm -23 "$work/routes.provider" "$work/provider")"
[ -z "$missing_provider" ] || {
  die "routes with no web provider method:"
  printf '%s\n' "$missing_provider" >&2
}

# Stale exemptions are a lie about the surface: fail on them too.
stale="$(comm -13 "$work/routes" "$work/exempt.all")"
[ -z "$stale" ] || {
  die "exempt/not-surfaced entries for routes that no longer exist:"
  printf '%s\n' "$stale" >&2
}

# ── Drum rule ─────────────────────────────────────────────────────
# Everything under /formations belongs to one of four orchestration
# verb groups: lifecycle, intent, guard/autonomy, membership.
# Lifecycle | intent | guard+autonomy | membership. `run-command` is the
# execution half of the `commands` group, not a fifth verb group.
ALLOWED='^/formations(/\{\})?$|^/formations/(deploy-team|eligible|intents|commands)$|^/formations/\{\}/(deploy|pause|resume|dissolve|rally|intent|cycle-intent|guard|toggle-guard|autonomy|cycle-autonomy|members|commands|run-command|eligible|propose-intent|votes)(/(\{\}|eligible))?$'
stray="$(grep '^/formations' "$work/routes" | grep -vE "$ALLOWED" || true)"
[ -z "$stray" ] || {
  die "formation routes outside the four orchestration verb groups:"
  printf '%s\n' "$stray" >&2
}

if [ "$fail" -ne 0 ]; then
  exit 1
fi

printf 'check-surface: %s routes, %s CLI paths, %s provider paths — OK\n' \
  "$(wc -l < "$work/routes" | tr -d ' ')" \
  "$(wc -l < "$work/verbs" | tr -d ' ')" \
  "$(wc -l < "$work/provider" | tr -d ' ')"
