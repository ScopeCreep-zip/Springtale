#!/usr/bin/env sh
# Print, one per line, every daemon route the command line calls.
#
# `springtale --help` cannot answer this: clap emits no machine-readable
# help, and a verb name ("rally") does not carry the route it hits. The
# CLI's path literals do, and they are the same contract the plan asks
# for — a route with no CLI path literal has no command-line verb.
#
# A literal is a route with two kinds of noise stripped, the same two
# the OpenAPI templates do not carry:
#
#   "/events?limit={limit}"        -> /events
#   "/formations/{id}/deploy"      -> /formations/{}/deploy
#
# A `{hole}` is a path segment only when a `/` introduces it; a hole
# anywhere else is an interpolated query string or base URL, not a
# segment, and is dropped rather than turned into `{}`.
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
grep -rhoE '"/[A-Za-z0-9_{}/.:?=&-]*"' "$root/apps/springtale-cli/src" \
  | tr -d '"' \
  | sed -e 's/?.*$//' \
        -e "s#/{[^}]*}#/%HOLE%#g" \
        -e "s/{[^}]*}//g" \
        -e "s/%HOLE%/{}/g" \
        -e 's#/\{1,\}$##' \
  | grep -vE '^/?$' \
  | sort -u
