#!/usr/bin/env bash
set -euo pipefail

version=""
if [[ "${1:-}" == "--version" ]]; then
  version="${2:-}"
else
  version="${1:-}"
fi
if [[ -z "$version" ]]; then
  echo "Usage: $0 --version <x.y.z>" >&2
  exit 2
fi
version="${version#v}"
tag="v$version"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Worktree is not clean. Commit or stash changes before publishing." >&2
  exit 1
fi

"$repo_root/scripts/assert-version.sh" "$version"
"$repo_root/scripts/assert-release-notes.sh" "$repo_root/docs/releases/v$version.md"
gh auth status

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "Local tag already exists: $tag" >&2
  exit 1
fi
if [[ -n "$(git ls-remote --tags origin "refs/tags/$tag")" ]]; then
  echo "Remote tag already exists: $tag" >&2
  exit 1
fi

git push origin HEAD
git tag "$tag"
git push origin "$tag"

echo "Release tag pushed: $tag"
echo "GitHub Actions will build and upload all desktop release assets."
