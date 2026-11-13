#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
expected="${1:-}"

EXPECTED_VERSION="$expected" node <<'NODE'
const fs = require('node:fs');

function readJson(path) {
  return JSON.parse(fs.readFileSync(path, 'utf8'));
}
function cargoManifestVersion(path) {
  const match = fs.readFileSync(path, 'utf8').match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) throw new Error(`Unable to read version from ${path}`);
  return match[1];
}
function cargoLockVersion(path, name) {
  const content = fs.readFileSync(path, 'utf8');
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = content.match(new RegExp(`\\[\\[package\\]\\]\\s+name\\s*=\\s*"${escaped}"\\s+version\\s*=\\s*"([^"]+)"`));
  if (!match) throw new Error(`Unable to read ${name} version from ${path}`);
  return match[1];
}

const packageJson = readJson('package.json');
const packageLock = readJson('package-lock.json');
const versions = new Map([
  ['package.json', packageJson.version],
  ['package-lock.json', packageLock.version],
  ['package-lock.json packages[""]', packageLock.packages?.['']?.version],
  ['src-tauri/Cargo.toml', cargoManifestVersion('src-tauri/Cargo.toml')],
  ['src-tauri/Cargo.lock', cargoLockVersion('src-tauri/Cargo.lock', 'acliv')],
  ['src-tauri/web/Cargo.toml', cargoManifestVersion('src-tauri/web/Cargo.toml')],
  ['src-tauri/web/Cargo.lock', cargoLockVersion('src-tauri/web/Cargo.lock', 'acliv-web')],
  ['src-tauri/tauri.conf.json', readJson('src-tauri/tauri.conf.json').version],
]);

const expected = process.env.EXPECTED_VERSION || packageJson.version;
for (const [path, actual] of versions) {
  if (actual !== expected) {
    throw new Error(`Version mismatch in ${path}: expected ${expected}, got ${actual ?? '<missing>'}`);
  }
}
console.log(`Version files aligned: ${expected}`);
NODE
