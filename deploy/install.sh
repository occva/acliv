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

has_session_data() {
  local home_dir="$1"
  [[ -d "$home_dir/.codex/sessions" ]] \
    || [[ -d "$home_dir/.claude/projects" ]] \
    || [[ -d "$home_dir/.gemini/tmp" ]] \
    || [[ -d "$home_dir/.openclaw/agents" ]] \
    || [[ -d "$home_dir/.config/opencode/storage/session" ]]
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

provider_dir_or_empty() {
  local path="$1"
  if [[ -d "$path" ]]; then
    echo "$path"
  else
    echo ""
  fi
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

  local deploy_mode
  deploy_mode="$(get_env_value AICHV_DEPLOY_MODE)"
  if [[ -z "$deploy_mode" ]]; then
    deploy_mode="${AICHV_DEPLOY_MODE:-image}"
    set_env_value "AICHV_DEPLOY_MODE" "$deploy_mode"
  fi

  local image
  image="$(get_env_value AICHV_IMAGE)"
  if [[ -z "$image" ]]; then
    image="${AICHV_IMAGE:-ghcr.io/occva/ai-cli-history-viewer}"
    set_env_value "AICHV_IMAGE" "$image"
  fi

  local version
  version="$(get_env_value AICHV_VERSION)"
  if [[ -z "$version" ]]; then
    version="${AICHV_VERSION:-latest}"
    set_env_value "AICHV_VERSION" "$version"
  fi

  local host_home
  host_home="$(get_env_value HOST_HOME)"
  local auto_home
  auto_home="$(resolve_host_home_default)"
  if [[ -n "${AICHV_HOST_HOME:-}" ]]; then
    host_home="$AICHV_HOST_HOME"
  elif [[ -z "$host_home" || "$host_home" == "/home/your-user" ]]; then
    host_home="$auto_home"
  elif [[ "$host_home" != "$auto_home" ]] \
    && ! has_session_data "$host_home" \
    && has_session_data "$auto_home"; then
    log_info "Detected session data under $auto_home, updating HOST_HOME."
    host_home="$auto_home"
  fi
  set_env_value "HOST_HOME" "$host_home"

  local claude_dir codex_dir gemini_dir openclaw_dir opencode_dir
  claude_dir="$(get_env_value CLAUDE_DIR)"
  codex_dir="$(get_env_value CODEX_DIR)"
  gemini_dir="$(get_env_value GEMINI_DIR)"
  openclaw_dir="$(get_env_value OPENCLAW_DIR)"
  opencode_dir="$(get_env_value OPENCODE_DIR)"

  if [[ -z "$claude_dir" ]]; then
    claude_dir="$(provider_dir_or_empty "$host_home/.claude")"
    set_env_value "CLAUDE_DIR" "$claude_dir"
  fi
  if [[ -z "$codex_dir" ]]; then
    codex_dir="$(provider_dir_or_empty "$host_home/.codex")"
    set_env_value "CODEX_DIR" "$codex_dir"
  fi
  if [[ -z "$gemini_dir" ]]; then
    gemini_dir="$(provider_dir_or_empty "$host_home/.gemini")"
    set_env_value "GEMINI_DIR" "$gemini_dir"
  fi
  if [[ -z "$openclaw_dir" ]]; then
    openclaw_dir="$(provider_dir_or_empty "$host_home/.openclaw")"
    set_env_value "OPENCLAW_DIR" "$openclaw_dir"
  fi
  if [[ -z "$opencode_dir" ]]; then
    opencode_dir="$(provider_dir_or_empty "$host_home/.config/opencode")"
    set_env_value "OPENCODE_DIR" "$opencode_dir"
  fi

  local run_as_root
  run_as_root="$(get_env_value AICHV_RUN_AS_ROOT)"
  if [[ -z "$run_as_root" ]]; then
    run_as_root="0"
    for path in "$claude_dir" "$codex_dir" "$gemini_dir" "$openclaw_dir" "$opencode_dir"; do
      if [[ -n "$path" && "$path" == /root/* ]]; then
        run_as_root="1"
        log_info "Detected root-owned provider directory $path, enabling AICHV_RUN_AS_ROOT=1."
        break
      fi
    done
    set_env_value "AICHV_RUN_AS_ROOT" "$run_as_root"
  fi

  local port
  port="$(get_env_value AICHV_PORT)"
  if [[ -z "$port" ]]; then
    port="17860"
    set_env_value "AICHV_PORT" "$port"
  fi

  chmod 600 .env
  mkdir -p app_data empty/claude empty/codex empty/gemini empty/openclaw empty/opencode
}

compose_up_image() {
  log_info "Pulling prebuilt image..."
  docker compose -f docker-compose.yml pull
  log_info "Starting service from prebuilt image..."
  docker compose -f docker-compose.yml up -d
}

compose_up_source() {
  log_info "Building image locally from source..."
  docker compose -f docker-compose.build.yml up -d --build
}

start_service() {
  cd "$INSTALL_DIR/deploy"

  local mode
  mode="$(get_env_value AICHV_DEPLOY_MODE)"
  mode="${mode:-image}"

  case "$mode" in
    image)
      compose_up_image
      ;;
    source)
      compose_up_source
      ;;
    auto)
      if compose_up_image; then
        log_info "Prebuilt image deployment succeeded."
      else
        log_warn "Prebuilt image deployment failed, falling back to source build."
        compose_up_source
      fi
      ;;
    *)
      log_error "Unsupported AICHV_DEPLOY_MODE: $mode"
      exit 1
      ;;
  esac

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
