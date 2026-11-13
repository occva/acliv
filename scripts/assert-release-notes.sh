#!/usr/bin/env bash
set -euo pipefail

notes_path="${1:-}"
if [[ -z "$notes_path" ]]; then
  echo "Usage: $0 <release-notes-path>" >&2
  exit 2
fi

NOTES_PATH="$notes_path" node <<'NODE'
const fs = require('node:fs');

const path = process.env.NOTES_PATH;
if (!fs.existsSync(path)) throw new Error(`Release notes file not found: ${path}`);
const content = fs.readFileSync(path, 'utf8');
if (!content.trim()) throw new Error(`Release notes file is empty: ${path}`);

const heading = content.match(/^## Changes\s*$/m);
if (!heading) throw new Error(`Release notes must include ## Changes: ${path}`);
const tail = content.slice(heading.index + heading[0].length);
const nextHeading = tail.search(/^##\s/m);
const changes = (nextHeading >= 0 ? tail.slice(0, nextHeading) : tail).trim();
if (!changes) {
  throw new Error(`Release notes must include a non-empty ## Changes section: ${path}`);
}
if (!/^更新内容:\s*$/m.test(changes) || !/^Updates:\s*$/m.test(changes)) {
  throw new Error(`Release notes must include bilingual user-facing changes: ${path}`);
}
if (/^\s*-\s*(feat|fix|refactor|docs|style|test|chore|ci|build)\([^)]+\):/mi.test(changes)) {
  throw new Error(`Release notes must be user-facing, not raw commit subjects: ${path}`);
}
if (/\bTODO\b/i.test(content)) {
  throw new Error(`Release notes are not publish-ready: found TODO in ${path}`);
}
console.log(`Release notes valid: ${path}`);
NODE
