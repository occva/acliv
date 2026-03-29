[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Version
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

function Invoke-Step {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Name,
    [Parameter(Mandatory = $true)]
    [scriptblock]$Action
  )

  Write-Host "==> $Name" -ForegroundColor Cyan
  $global:LASTEXITCODE = 0
  & $Action
  if ($LASTEXITCODE -ne 0) {
    throw "Step failed with exit code ${LASTEXITCODE}: $Name"
  }
}

$tag = "v$Version"
$releaseDir = Join-Path $RepoRoot "release\$tag"
$desktopExe = Join-Path $releaseDir "ai-cli-history-viewer-$tag.exe"
$setupExe = Join-Path $releaseDir "ai-cli-history-viewer-$tag-x64-setup.exe"
$msiFile = Join-Path $releaseDir "ai-cli-history-viewer-$tag-x64-en-us.msi"
$releaseNotes = Join-Path $releaseDir "release-notes-$tag.md"

Invoke-Step -Name 'verify gh auth' -Action {
  gh auth status
}

foreach ($path in @($desktopExe, $setupExe, $msiFile, $releaseNotes)) {
  if (-not (Test-Path $path)) {
    throw "Missing release artifact: $path"
  }
}

Invoke-Step -Name 'ensure git tag exists' -Action {
  $existingTag = git tag --list $tag
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to inspect existing tag: $tag"
  }
  if (-not ($existingTag | Where-Object { $_ -eq $tag })) {
    git tag $tag
  }
}

Invoke-Step -Name 'push commits' -Action {
  git push
}

Invoke-Step -Name 'push tag' -Action {
  git push origin $tag
}

$releaseExists = $false
gh release view $tag --repo occva/ai-cli-history-viewer --json tagName *> $null
if ($LASTEXITCODE -eq 0) {
  $releaseExists = $true
}

if (-not $releaseExists) {
  Invoke-Step -Name 'create GitHub release' -Action {
    gh release create $tag `
      $desktopExe `
      $setupExe `
      $msiFile `
      --repo occva/ai-cli-history-viewer `
      --notes-file $releaseNotes `
      --title $tag
  }
}
else {
  Invoke-Step -Name 'upload release assets' -Action {
    gh release upload $tag `
      $desktopExe `
      $setupExe `
      $msiFile `
      --repo occva/ai-cli-history-viewer `
      --clobber
  }
}

Write-Host "GitHub release published: $tag" -ForegroundColor Green
