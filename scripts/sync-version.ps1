[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Version
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

if ($Version.StartsWith('v')) {
  $Version = $Version.Substring(1)
}

function Write-Utf8NoBom {
  param(
    [string]$FilePath,
    [string]$Content
  )

  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($FilePath, $Content, $utf8NoBom)
}

function Set-JsonVersion {
  param(
    [string]$FilePath,
    [string]$NewVersion
  )

  $content = Get-Content $FilePath -Raw
  $updated = [regex]::Replace($content, '(?m)^(\s*"version"\s*:\s*")[^"]+(")', "`${1}$NewVersion`${2}", 1)
  if ($updated -eq $content) {
    if ($content -notmatch ('(?m)^\s*"version"\s*:\s*"' + [regex]::Escape($NewVersion) + '"')) {
      throw "Failed to update version in $FilePath"
    }
    return
  }

  Write-Utf8NoBom -FilePath $FilePath -Content $updated
}

function Set-PackageLockVersion {
  param(
    [string]$FilePath,
    [string]$NewVersion
  )

  $content = Get-Content $FilePath -Raw
  $updated = [regex]::Replace(
    $content,
    '(?s)\A(\s*\{\s*"name"\s*:\s*"acliv"\s*,\s*"version"\s*:\s*")[^"]+(")',
    "`${1}$NewVersion`${2}",
    1
  )
  $updated = [regex]::Replace(
    $updated,
    '(?s)("packages"\s*:\s*\{\s*""\s*:\s*\{\s*"name"\s*:\s*"acliv"\s*,\s*"version"\s*:\s*")[^"]+(")',
    "`${1}$NewVersion`${2}",
    1
  )

  if ($updated -eq $content) {
    if (
      $content -notmatch ('(?s)\A\s*\{\s*"name"\s*:\s*"acliv"\s*,\s*"version"\s*:\s*"' + [regex]::Escape($NewVersion) + '"') -or
      $content -notmatch ('(?s)"packages"\s*:\s*\{\s*""\s*:\s*\{\s*"name"\s*:\s*"acliv"\s*,\s*"version"\s*:\s*"' + [regex]::Escape($NewVersion) + '"')
    ) {
      throw "Failed to update version in $FilePath"
    }
    return
  }

  Write-Utf8NoBom -FilePath $FilePath -Content $updated
}

function Set-CargoVersion {
  param([string]$FilePath, [string]$NewVersion)

  $content = Get-Content $FilePath -Raw
  $updated = [regex]::Replace($content, '(?m)^(\s*version\s*=\s*")[^"]+(")', "`${1}$NewVersion`${2}", 1)
  if ($updated -eq $content) {
    if ($content -notmatch ('(?m)^\s*version\s*=\s*"' + [regex]::Escape($NewVersion) + '"')) {
      throw "Failed to update Cargo version in $FilePath"
    }
    return
  }
  Write-Utf8NoBom -FilePath $FilePath -Content $updated
}

function Set-CargoLockPackageVersion {
  param(
    [string]$FilePath,
    [string]$PackageName,
    [string]$NewVersion
  )

  $content = Get-Content $FilePath -Raw
  $escapedPackageName = [regex]::Escape($PackageName)
  $pattern = "(?ms)(\[\[package\]\]\s+name\s*=\s*`"$escapedPackageName`"\s+version\s*=\s*`")[^`"]+(`")"
  $updated = [regex]::Replace($content, $pattern, "`${1}$NewVersion`${2}", 1)
  if ($updated -eq $content) {
    if ($content -notmatch "(?ms)\[\[package\]\]\s+name\s*=\s*`"$escapedPackageName`"\s+version\s*=\s*`"$([regex]::Escape($NewVersion))`"") {
      throw "Failed to update Cargo lock package version in $FilePath for $PackageName"
    }
    return
  }
  Write-Utf8NoBom -FilePath $FilePath -Content $updated
}

$packageJson = Join-Path $RepoRoot 'package.json'
$packageLock = Join-Path $RepoRoot 'package-lock.json'
$tauriDir = Join-Path $RepoRoot 'src-tauri'
$webDir = Join-Path $tauriDir 'web'
$cargoToml = Join-Path $tauriDir 'Cargo.toml'
$cargoLock = Join-Path $tauriDir 'Cargo.lock'
$webCargoToml = Join-Path $webDir 'Cargo.toml'
$webCargoLock = Join-Path $webDir 'Cargo.lock'
$tauriConfig = Join-Path $tauriDir 'tauri.conf.json'

Set-JsonVersion -FilePath $packageJson -NewVersion $Version
if (Test-Path $packageLock) {
  Set-PackageLockVersion -FilePath $packageLock -NewVersion $Version
}
Set-CargoVersion -FilePath $cargoToml -NewVersion $Version
Set-CargoLockPackageVersion -FilePath $cargoLock -PackageName 'acliv' -NewVersion $Version
Set-CargoVersion -FilePath $webCargoToml -NewVersion $Version
Set-CargoLockPackageVersion -FilePath $webCargoLock -PackageName 'acliv-web' -NewVersion $Version
Set-JsonVersion -FilePath $tauriConfig -NewVersion $Version

Write-Host "Version files synced: $Version" -ForegroundColor Green
