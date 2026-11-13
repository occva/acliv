#!/usr/bin/env bash
set -euo pipefail

scope="local"
if [[ "${1:-}" == "--scope" ]]; then
  scope="${2:-}"
elif [[ -n "${1:-}" ]]; then
  scope="$1"
fi
case "$scope" in
  local|push|release) ;;
  *) echo "Usage: $0 [--scope local|push|release]" >&2; exit 2 ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

run_step() {
  local name="$1"
  shift
  echo "==> $name"
  "$@"
}

run_step "svelte-check" npx svelte-check
run_step "cargo check (desktop)" cargo check --manifest-path src-tauri/Cargo.toml
if [[ "$scope" == "push" || "$scope" == "release" ]]; then
  run_step "cargo test" cargo test --manifest-path src-tauri/Cargo.toml
fi
if [[ "$scope" == "release" ]]; then
  run_step "cargo check (web)" cargo check --manifest-path src-tauri/web/Cargo.toml
fi

echo "Check complete: $scope"
