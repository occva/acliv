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

  if [ "$(id -u)" = "0" ] && [ "${ACLIV_RUN_AS_ROOT:-0}" != "1" ]; then
    if check_dir_access_as app "$path"; then
      return 0
    fi

    log_error "$label data directory is not readable by app user: $path"
    log_error "Set ACLIV_RUN_AS_ROOT=1 or mount a readable provider directory."
    exit 1
  fi

  if [ ! -r "$path" ] || [ ! -x "$path" ]; then
    log_error "$label data directory is not readable by current user: $path"
    exit 1
  fi
}

if [ -z "${ACLIV_WEB_USERNAME:-}" ]; then
  export ACLIV_WEB_USERNAME="admin"
fi

if [ -z "${ACLIV_WEB_PASSWORD:-}" ] && [ -n "${ACLIV_TOKEN:-}" ]; then
  log_warn "ACLIV_WEB_PASSWORD is unset; falling back to legacy ACLIV_TOKEN login password."
elif [ -z "${ACLIV_WEB_PASSWORD:-}" ]; then
  log_warn "ACLIV_WEB_PASSWORD is unset; acliv-web will generate a temporary password at startup."
fi

for provider in \
  "Claude:${ACLIV_CLAUDE_DIR:-}" \
  "Codex:${ACLIV_CODEX_DIR:-}" \
  "Gemini:${ACLIV_GEMINI_DIR:-}" \
  "OpenClaw:${ACLIV_OPENCLAW_DIR:-}" \
  "OpenCode:${ACLIV_OPENCODE_DIR:-}"
do
  label="${provider%%:*}"
  path="${provider#*:}"
  check_provider_dir "$label" "$path"
done

if [ "$(id -u)" = "0" ]; then
  mkdir -p /app/data
  chown -R app:app /app 2>/dev/null || true

  if [ "${ACLIV_RUN_AS_ROOT:-0}" = "1" ]; then
    log_warn "Running acliv-web as root because ACLIV_RUN_AS_ROOT=1."
  else
    exec su-exec app "$0" "$@"
  fi
fi

if [ "${1#-}" != "$1" ]; then
  set -- /app/acliv-web "$@"
fi

exec "$@"
