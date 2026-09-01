#!/bin/sh
set -eu

service=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    -u)
      shift
      service="${1:-}"
      ;;
  esac
  shift
done

if [ "$service" != 'runory-fixture.service' ]; then
  exit 1
fi

printf '%s\n' \
  'Aug 30 10:00:00 runory fixture[1]: service state recorded' \
  "Aug 30 10:00:01 runory fixture[1]: state=$(cat /etc/runory-service-state 2>/dev/null || printf '%s' 'active')"
