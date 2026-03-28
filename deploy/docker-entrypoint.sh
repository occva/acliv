#!/bin/sh
set -e

log_info() {
  echo "[INFO] $*"
}

log_warn() {
  echo "[WARN] $*"
}

log_error() {
  echo "[ERROR] $*" >&2
}

check_dir_access_as() {
  user="$1"
  path="$2"
  su-exec "$user" sh -c "test -r \"$path\" && test -x \"$path\""
}

check_provider_dir() {
  label="$1"
  path="$2"

  if [ -z "$path" ]; then
    return 0
  fi

  if [ ! -e "$path" ]; then
    log_info "$label data directory not mounted: $path"
    return 0
  fi

  if [ "$(id -u)" = "0" ] && [ "${AICHV_RUN_AS_ROOT:-0}" != "1" ]; then
    if check_dir_access_as app "$path"; then
      return 0
    fi

    log_error "$label data directory is not readable by app user: $path"
    log_error "Set AICHV_RUN_AS_ROOT=1 or mount a readable provider directory."
    exit 1
  fi

  if [ ! -r "$path" ] || [ ! -x "$path" ]; then
    log_error "$label data directory is not readable by current user: $path"
    exit 1
  fi
}

if [ -z "${AICHV_TOKEN:-}" ]; then
  log_error "AICHV_TOKEN is required"
  exit 1
fi

for provider in \
  "Claude:${AICHV_CLAUDE_DIR:-}" \
  "Codex:${AICHV_CODEX_DIR:-}" \
  "Gemini:${AICHV_GEMINI_DIR:-}" \
  "OpenClaw:${AICHV_OPENCLAW_DIR:-}" \
  "OpenCode:${AICHV_OPENCODE_DIR:-}"
do
  label="${provider%%:*}"
  path="${provider#*:}"
  check_provider_dir "$label" "$path"
done

if [ "$(id -u)" = "0" ]; then
  mkdir -p /app/data
  chown -R app:app /app 2>/dev/null || true

  if [ "${AICHV_RUN_AS_ROOT:-0}" = "1" ]; then
    log_warn "Running aichv-web as root because AICHV_RUN_AS_ROOT=1."
  else
    exec su-exec app "$0" "$@"
  fi
fi

if [ "${1#-}" != "$1" ]; then
  set -- /app/aichv-web "$@"
fi

exec "$@"
