#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${AICHV_REPO_URL:-https://github.com/occva/ai-cli-history-viewer.git}"
BRANCH="${AICHV_REPO_BRANCH:-master}"

if [[ "${EUID}" -eq 0 ]]; then
  INSTALL_DIR="${AICHV_INSTALL_DIR:-/opt/ai-cli-history-viewer}"
else
  INSTALL_DIR="${AICHV_INSTALL_DIR:-$HOME/ai-cli-history-viewer}"
fi

log_info() {
  echo "[INFO] $*"
}

log_warn() {
  echo "[WARN] $*"
}

log_error() {
  echo "[ERROR] $*" >&2
}

has_session_data() {
  local home_dir="$1"
  [[ -d "$home_dir/.codex/sessions" ]] \
    || [[ -d "$home_dir/.claude/projects" ]] \
    || [[ -d "$home_dir/.gemini/tmp" ]] \
    || [[ -d "$home_dir/.openclaw/agents" ]] \
    || [[ -d "$home_dir/.config/opencode/storage/session" ]]
}

need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    log_error "Missing required command: $cmd"
    exit 1
  fi
}

ensure_dependencies() {
  need_cmd git
  need_cmd docker
  if ! docker compose version >/dev/null 2>&1; then
    log_error "Docker Compose v2 is required (docker compose)."
    exit 1
  fi
}

sync_repo() {
  if [[ ! -d "$INSTALL_DIR/.git" ]]; then
    log_info "Cloning repository to $INSTALL_DIR"
    git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$INSTALL_DIR"
    return
  fi

  log_info "Updating repository in $INSTALL_DIR"
  if ! git -C "$INSTALL_DIR" pull --ff-only; then
    log_warn "git pull failed, keep local repo as-is."
  fi
}

generate_secret() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
    return
  fi
  if command -v xxd >/dev/null 2>&1; then
    head -c 32 /dev/urandom | xxd -p -c 32
    return
  fi
  log_error "openssl or xxd is required to generate AICHV_TOKEN."
  exit 1
}

get_env_value() {
  local key="$1"
  if [[ -f .env ]]; then
    grep -E "^${key}=" .env | head -n 1 | cut -d '=' -f 2- || true
  fi
}

set_env_value() {
  local key="$1"
  local value="$2"
  if grep -qE "^${key}=" .env; then
    sed -i "s|^${key}=.*|${key}=${value}|" .env
  else
    echo "${key}=${value}" >> .env
  fi
}

resolve_host_home_default() {
  if [[ -n "${AICHV_HOST_HOME:-}" ]]; then
    echo "$AICHV_HOST_HOME"
    return
  fi

  local -a candidates=()
  if [[ -n "${HOME:-}" ]]; then
    candidates+=("$HOME")
  fi
  if [[ -n "${SUDO_USER:-}" && "${SUDO_USER}" != "root" ]]; then
    candidates+=("$(eval echo "~${SUDO_USER}")")
  fi
  if [[ "${EUID}" -eq 0 ]]; then
    candidates+=("/root")
  fi

  local candidate
  local fallback=""
  for candidate in "${candidates[@]}"; do
    [[ -z "$candidate" || ! -d "$candidate" ]] && continue
    [[ -z "$fallback" ]] && fallback="$candidate"
    if has_session_data "$candidate"; then
      echo "$candidate"
      return
    fi
  done

  if [[ -n "$fallback" ]]; then
    echo "$fallback"
  else
    echo "$HOME"
  fi
}

resolve_access_host() {
  if [[ -n "${AICHV_ACCESS_HOST:-}" ]]; then
    echo "$AICHV_ACCESS_HOST"
    return
  fi

  if command -v hostname >/dev/null 2>&1; then
    local lan_ip
    lan_ip="$(hostname -I 2>/dev/null | awk '{for (i=1; i<=NF; i++) if ($i !~ /^127\./) {print $i; exit}}')"
    if [[ -n "$lan_ip" ]]; then
      echo "$lan_ip"
      return
    fi
  fi

  if command -v ip >/dev/null 2>&1; then
    local route_ip
    route_ip="$(ip route get 1.1.1.1 2>/dev/null | awk '{for (i=1; i<=NF; i++) if ($i == "src") {print $(i+1); exit}}')"
    if [[ -n "$route_ip" ]]; then
      echo "$route_ip"
      return
    fi
  fi

  echo "localhost"
}

prepare_env() {
  cd "$INSTALL_DIR/deploy"

  if [[ ! -f .env ]]; then
    cp .env.example .env
  fi

  local token
  token="$(get_env_value AICHV_TOKEN)"
  if [[ -z "$token" ]]; then
    token="$(generate_secret)"
    set_env_value "AICHV_TOKEN" "$token"
  fi

  local host_home
  host_home="$(get_env_value HOST_HOME)"
  local auto_home
  auto_home="$(resolve_host_home_default)"
  if [[ -n "${AICHV_HOST_HOME:-}" ]]; then
    host_home="$AICHV_HOST_HOME"
    set_env_value "HOST_HOME" "$host_home"
  elif [[ -z "$host_home" || "$host_home" == "/home/your-user" ]]; then
    host_home="$auto_home"
    set_env_value "HOST_HOME" "$host_home"
  elif [[ "$host_home" != "$auto_home" ]] \
    && ! has_session_data "$host_home" \
    && has_session_data "$auto_home"; then
    log_info "Detected session data under $auto_home, updating HOST_HOME."
    host_home="$auto_home"
    set_env_value "HOST_HOME" "$host_home"
  fi

  local port
  port="$(get_env_value AICHV_PORT)"
  if [[ -z "$port" ]]; then
    port="17860"
    set_env_value "AICHV_PORT" "$port"
  fi

  chmod 600 .env
  mkdir -p app_data
}

start_service() {
  cd "$INSTALL_DIR/deploy"
  log_info "Starting service with docker compose..."
  docker compose -f docker-compose.local.yml up -d --build

  local token port access_host
  token="$(get_env_value AICHV_TOKEN)"
  port="$(get_env_value AICHV_PORT)"
  access_host="$(resolve_access_host)"

  echo ""
  echo "Installation complete."
  echo "Access URL:"
  echo "http://${access_host}:${port}/?token=${token}"
  if [[ "$access_host" != "localhost" ]]; then
    echo "Local URL: http://localhost:${port}/?token=${token}"
  fi
  echo ""
}

main() {
  ensure_dependencies
  sync_repo
  prepare_env
  start_service
}

main "$@"
