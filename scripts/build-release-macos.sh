#!/usr/bin/env bash
set -euo pipefail

version=""
allow_dirty=0
release_notes_path=""
unsigned=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) version="${2:-}"; shift 2 ;;
    --allow-dirty) allow_dirty=1; shift ;;
    --unsigned) unsigned=1; shift ;;
    --release-notes-path) release_notes_path="${2:-}"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 --version <x.y.z> [--allow-dirty] [--unsigned] [--release-notes-path <path>]"
      exit 0
      ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
if [[ -z "$version" ]]; then
  echo "--version is required" >&2
  exit 2
fi
version="${version#v}"
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This build script requires macOS." >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [[ "$allow_dirty" -ne 1 && -n "$(git status --porcelain)" ]]; then
  echo "Worktree is not clean. Commit or stash changes, or use --allow-dirty." >&2
  exit 1
fi

release_dir="$repo_root/release/v$version"
notes_source="${release_notes_path:-$repo_root/docs/releases/v$version.md}"
"$repo_root/scripts/sync-version.sh" "$version"
"$repo_root/scripts/assert-release-notes.sh" "$notes_source"

if [[ "$unsigned" -ne 1 && -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  key_path="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.acliv/tauri-updater.key}"
  if [[ ! -f "$key_path" ]]; then
    echo "Tauri updater signing key is required. Set TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH, or use --unsigned for a local DMG rehearsal." >&2
    exit 1
  fi
  export TAURI_SIGNING_PRIVATE_KEY="$(<"$key_path")"
  echo "Using local Tauri updater signing key: $key_path"
fi

rm -rf "$repo_root/dist" "$repo_root/src-tauri/target/release" "$release_dir"
mkdir -p "$release_dir"

if [[ "$unsigned" -eq 1 ]]; then
  npm run tauri build -- --bundles app,dmg --config '{"bundle":{"createUpdaterArtifacts":false}}'
else
  npm run tauri build -- --bundles app,dmg
fi

arch="$(uname -m)"
case "$arch" in
  arm64) release_arch="arm64" ;;
  x86_64) release_arch="x64" ;;
  *) echo "Unsupported macOS architecture: $arch" >&2; exit 1 ;;
esac

dmg_path="$(find src-tauri/target/release/bundle/dmg -maxdepth 1 -type f -name '*.dmg' -print -quit)"
tar_path=""
if [[ "$unsigned" -ne 1 ]]; then
  tar_path="$(find src-tauri/target/release/bundle/macos -maxdepth 1 -type f -name '*.tar.gz' -print -quit)"
fi
if [[ -z "$dmg_path" || ! -f "$dmg_path" ]]; then
  echo "Missing macOS DMG installer." >&2
  exit 1
fi
if [[ "$unsigned" -ne 1 && ( -z "$tar_path" || ! -f "$tar_path" || ! -f "$tar_path.sig" ) ]]; then
  echo "Missing macOS updater tarball or signature." >&2
  exit 1
fi

cp "$dmg_path" "$release_dir/acliv-v$version-macos-$release_arch.dmg"
if [[ "$unsigned" -ne 1 ]]; then
  cp "$tar_path" "$release_dir/acliv-v$version-macos-$release_arch.tar.gz"
  cp "$tar_path.sig" "$release_dir/acliv-v$version-macos-$release_arch.tar.gz.sig"
fi
cp "$notes_source" "$release_dir/release-notes-v$version.md"
"$repo_root/scripts/assert-release-notes.sh" "$release_dir/release-notes-v$version.md"

echo "macOS release artifacts ready: $release_dir"
