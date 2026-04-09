#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${ACLIV_REPO_URL:-https://github.com/occva/acliv.git}"
BRANCH="${ACLIV_REPO_BRANCH:-master}"

if [[ "${EUID}" -eq 0 ]]; then
  INSTALL_DIR="${ACLIV_INSTALL_DIR:-/opt/acliv}"
else
  INSTALL_DIR="${ACLIV_INSTALL_DIR:-$HOME/acliv}"
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
  need_cmd curl
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
  local bytes="${1:-16}"
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex "$bytes"
    return
  fi
  if command -v xxd >/dev/null 2>&1; then
    head -c "$bytes" /dev/urandom | xxd -p -c "$bytes"
    return
  fi
  log_error "openssl or xxd is required to generate ACLIV_WEB_PASSWORD."
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
  if [[ -n "${ACLIV_HOST_HOME:-}" ]]; then
    echo "$ACLIV_HOST_HOME"
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

resolve_public_ip() {
  local public_ip
  public_ip="$(curl -fsSL --connect-timeout 5 --max-time 10 ifconfig.me/ip 2>/dev/null | tr -d '\r\n' || true)"
  if [[ -n "$public_ip" ]]; then
    echo "$public_ip"
  fi
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

  local web_username
  web_username="$(get_env_value ACLIV_WEB_USERNAME)"
  if [[ -z "$web_username" ]]; then
    web_username="admin"
    set_env_value "ACLIV_WEB_USERNAME" "$web_username"
  fi

  local web_password
  web_password="$(get_env_value ACLIV_WEB_PASSWORD)"
  if [[ -z "$web_password" ]]; then
    web_password="$(generate_secret 16)"
    set_env_value "ACLIV_WEB_PASSWORD" "$web_password"
  fi

  local auth_token
  auth_token="$(get_env_value ACLIV_TOKEN)"
  if [[ -z "$auth_token" ]]; then
    auth_token="$(generate_secret 32)"
    set_env_value "ACLIV_TOKEN" "$auth_token"
  fi

  local image
  image="$(get_env_value ACLIV_IMAGE)"
  if [[ -z "$image" ]]; then
    image="${ACLIV_IMAGE:-ghcr.io/occva/acliv}"
    set_env_value "ACLIV_IMAGE" "$image"
  fi

  local version
  version="$(get_env_value ACLIV_VERSION)"
  if [[ -z "$version" ]]; then
    version="${ACLIV_VERSION:-latest}"
    set_env_value "ACLIV_VERSION" "$version"
  fi

  local host_home
  host_home="$(get_env_value HOST_HOME)"
  local auto_home
  auto_home="$(resolve_host_home_default)"
  if [[ -n "${ACLIV_HOST_HOME:-}" ]]; then
    host_home="$ACLIV_HOST_HOME"
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
  run_as_root="$(get_env_value ACLIV_RUN_AS_ROOT)"
  if [[ -z "$run_as_root" ]]; then
    run_as_root="0"
    for path in "$claude_dir" "$codex_dir" "$gemini_dir" "$openclaw_dir" "$opencode_dir"; do
      if [[ -n "$path" && "$path" == /root/* ]]; then
        run_as_root="1"
        log_info "Detected root-owned provider directory $path, enabling ACLIV_RUN_AS_ROOT=1."
        break
      fi
    done
    set_env_value "ACLIV_RUN_AS_ROOT" "$run_as_root"
  fi

  local port
  port="$(get_env_value ACLIV_PORT)"
  if [[ -z "$port" ]]; then
    port="17860"
    set_env_value "ACLIV_PORT" "$port"
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

start_service() {
  cd "$INSTALL_DIR/deploy"
  compose_up_image

  local web_username web_password port public_ip
  web_username="$(get_env_value ACLIV_WEB_USERNAME)"
  web_password="$(get_env_value ACLIV_WEB_PASSWORD)"
  port="$(get_env_value ACLIV_PORT)"
  public_ip="$(resolve_public_ip)"

  echo ""
  echo "Installation complete."
  if [[ -n "$public_ip" ]]; then
    echo "Access URL:"
    echo "http://${public_ip}:${port}/"
  else
    echo "未获取到公网 IP，请手动替换 <your-server-ip> 后访问："
    echo "http://<your-server-ip>:${port}/"
  fi
  echo "Username: ${web_username}"
  echo "Password: ${web_password}"
  echo ""
}

main() {
  ensure_dependencies
  sync_repo
  prepare_env
  start_service
}

main "$@"
