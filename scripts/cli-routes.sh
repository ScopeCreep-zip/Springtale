#!/usr/bin/env sh
# Print, one per line, every daemon route the command line calls.
#
# `springtale --help` cannot answer this: clap emits no machine-readable
# help, and a verb name ("rally") does not carry the route it hits. The
# CLI's path literals do, and they are the same contract the plan asks
# for — a route with no CLI path literal has no command-line verb.
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
grep -rhoE '"/[A-Za-z0-9_{}/.:-]*"' "$root/apps/springtale-cli/src" \
  | tr -d '"' \
  | sed -e 's/{[^}]*}/{}/g' -e 's#/\{1,\}$##' \
  | grep -vE '^/$' \
  | sort -u
