#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
if [[ -z "$version" ]]; then
  echo "Usage: $0 <version>" >&2
  exit 2
fi
version="${version#v}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
  echo "Invalid version: $version" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

VERSION="$version" node <<'NODE'
const fs = require('node:fs');

const version = process.env.VERSION;

function update(path, transform) {
  const original = fs.readFileSync(path, 'utf8');
  const updated = transform(original);
  if (updated === original && !original.includes(version)) {
    throw new Error(`Failed to update version in ${path}`);
  }
  fs.writeFileSync(path, updated, 'utf8');
}

function replaceFirst(content, pattern, label) {
  if (!pattern.test(content)) {
    throw new Error(`Unable to find ${label}`);
  }
  pattern.lastIndex = 0;
  return content.replace(pattern, `$1${version}$2`);
}

update('package.json', content =>
  replaceFirst(content, /^(\s*"version"\s*:\s*")[^"]+(")/m, 'package.json version')
);

update('package-lock.json', content => {
  let updated = replaceFirst(
    content,
    /^(\s*\{\s*"name"\s*:\s*"acliv"\s*,\s*"version"\s*:\s*")[^"]+(")/s,
    'package-lock root version',
  );
  updated = replaceFirst(
    updated,
    /("packages"\s*:\s*\{\s*""\s*:\s*\{\s*"name"\s*:\s*"acliv"\s*,\s*"version"\s*:\s*")[^"]+(")/s,
    'package-lock workspace version',
  );
  return updated;
});

for (const path of ['src-tauri/Cargo.toml', 'src-tauri/web/Cargo.toml']) {
  update(path, content =>
    replaceFirst(content, /^(\s*version\s*=\s*")[^"]+(")/m, `${path} package version`)
  );
}

for (const [path, name] of [
  ['src-tauri/Cargo.lock', 'acliv'],
  ['src-tauri/web/Cargo.lock', 'acliv-web'],
]) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const pattern = new RegExp(`(\\[\\[package\\]\\]\\s+name\\s*=\\s*"${escaped}"\\s+version\\s*=\\s*")[^"]+(")`);
  update(path, content => replaceFirst(content, pattern, `${path} ${name} version`));
}

update('src-tauri/tauri.conf.json', content =>
  replaceFirst(content, /^(\s*"version"\s*:\s*")[^"]+(")/m, 'Tauri config version')
);
NODE

"$repo_root/scripts/assert-version.sh" "$version"
echo "Version files synced: $version"
