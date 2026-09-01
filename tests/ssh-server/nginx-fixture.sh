#!/bin/sh
set -eu

if [ "${1:-}" != '-t' ]; then
  exit 2
fi

# Keep the read-only probe in flight briefly so cancellation is exercised over real SSH.
sleep 1
printf '%s\n' \
  'nginx: the configuration file /etc/nginx/nginx.conf syntax is ok' \
  'nginx: configuration file /etc/nginx/nginx.conf test is successful' >&2
