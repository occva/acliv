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

function Assert-Version {
  param(
    [string]$FilePath,
    [string]$Pattern,
    [string]$ExpectedVersion
  )

  $content = Get-Content $FilePath -Raw
  if ($content -notmatch $Pattern) {
    throw "Unable to read version from $FilePath"
  }
  $actualVersion = $Matches[1]
  if ($actualVersion -ne $ExpectedVersion) {
    throw "Version mismatch in $FilePath. Expected $ExpectedVersion, got $actualVersion."
  }
}

if ($Version.StartsWith('v')) {
  $Version = $Version.Substring(1)
}
$tag = "v$Version"

Invoke-Step -Name 'verify clean worktree' -Action {
  $status = git status --porcelain
  if ($LASTEXITCODE -ne 0) {
    throw 'Unable to read git status.'
  }
  if ($status) {
    throw 'Worktree is not clean. Commit or stash changes before publishing a release tag.'
  }
}

Invoke-Step -Name 'verify version files' -Action {
  Assert-Version -FilePath 'package.json' -Pattern '"version"\s*:\s*"([^"]+)"' -ExpectedVersion $Version
  Assert-Version -FilePath 'package-lock.json' -Pattern '"version"\s*:\s*"([^"]+)"' -ExpectedVersion $Version
  Assert-Version -FilePath 'src-tauri\Cargo.toml' -Pattern '(?m)^version\s*=\s*"([^"]+)"' -ExpectedVersion $Version
  Assert-Version -FilePath 'src-tauri\Cargo.lock' -Pattern '(?ms)\[\[package\]\]\s+name\s*=\s*"acliv"\s+version\s*=\s*"([^"]+)"' -ExpectedVersion $Version
  Assert-Version -FilePath 'src-tauri\web\Cargo.toml' -Pattern '(?m)^version\s*=\s*"([^"]+)"' -ExpectedVersion $Version
  Assert-Version -FilePath 'src-tauri\web\Cargo.lock' -Pattern '(?ms)\[\[package\]\]\s+name\s*=\s*"acliv-web"\s+version\s*=\s*"([^"]+)"' -ExpectedVersion $Version
  Assert-Version -FilePath 'src-tauri\tauri.conf.json' -Pattern '"version"\s*:\s*"([^"]+)"' -ExpectedVersion $Version
}

Invoke-Step -Name 'verify gh auth' -Action {
  gh auth status
}

Invoke-Step -Name 'ensure release tag is new' -Action {
  $existingLocalTag = git tag --list $tag
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to inspect local tags."
  }
  if ($existingLocalTag | Where-Object { $_ -eq $tag }) {
    throw "Local tag already exists: $tag"
  }

  $existingRemoteTag = git ls-remote --tags origin "refs/tags/$tag"
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to inspect remote tags."
  }
  if ($existingRemoteTag) {
    throw "Remote tag already exists: $tag"
  }
}

Invoke-Step -Name 'push commits' -Action {
  git push origin HEAD
}

Invoke-Step -Name 'create release tag' -Action {
  git tag $tag
}

Invoke-Step -Name 'push release tag' -Action {
  git push origin $tag
}

Write-Host "Release tag pushed: $tag" -ForegroundColor Green
Write-Host "GitHub Actions will build and upload all desktop release assets." -ForegroundColor Green
