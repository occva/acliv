[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Path
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not (Test-Path -LiteralPath $Path)) {
  throw "Release notes file not found: $Path"
}

$content = Get-Content -LiteralPath $Path -Raw -Encoding UTF8
if (-not $content.Trim()) {
  throw "Release notes file is empty: $Path"
}

$changesMatch = [regex]::Match(
  $content,
  '(?ms)^## Changes\s*(?<body>.*?)(?=^##\s|\z)'
)
if (-not $changesMatch.Success -or -not $changesMatch.Groups['body'].Value.Trim()) {
  throw "Release notes must include a non-empty ## Changes section: $Path"
}

function Decode-ReleaseNoteText {
  param([string]$Base64)

  return [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($Base64))
}

$changes = $changesMatch.Groups['body'].Value
$chineseHeading = [regex]::Escape((Decode-ReleaseNoteText '5pu05paw5YaF5a65Og=='))
if ($changes -notmatch "(?m)^$chineseHeading\s*$" -or $changes -notmatch '(?m)^Updates:\s*$') {
  throw "Release notes must include bilingual user-facing changes under the Chinese and English headings: $Path"
}

if ([regex]::IsMatch($changes, '(?m)^\s*-\s*(feat|fix|refactor|docs|style|test|chore|ci|build)\([^)]+\):')) {
  throw "Release notes must be user-facing, not raw commit subjects: $Path"
}

if ([regex]::IsMatch($content, '(?i)\bTODO\b')) {
  throw "Release notes are not publish-ready: found TODO placeholder text in $Path"
}
