#!/bin/sh
set -e

if [ -z "${AICHV_TOKEN:-}" ]; then
  echo "ERROR: AICHV_TOKEN is required"
  exit 1
fi

if [ "$(id -u)" = "0" ]; then
  mkdir -p /app/data
  chown -R app:app /app 2>/dev/null || true
  exec su-exec app "$0" "$@"
fi

if [ "${1#-}" != "$1" ]; then
  set -- /app/aichv-web "$@"
fi

exec "$@"
