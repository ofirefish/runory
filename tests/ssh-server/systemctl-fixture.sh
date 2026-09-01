#!/bin/sh
set -eu

if [ "${1:-}" = "is-active" ] && [ "${2:-}" = "runory-fixture.service" ]; then
  state="$(cat /etc/runory-service-state 2>/dev/null || printf '%s' 'active')"
  printf '%s\n' "$state"
  if [ "$state" = 'active' ]; then
    exit 0
  fi
  exit 3
fi

printf '%s\n' 'inactive'
exit 3
