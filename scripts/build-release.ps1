[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Version,
  [switch]$AllowDirty
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

function Assert-CleanWorktree {
  if ($AllowDirty) {
    return
  }

  $status = git status --porcelain
  if ($LASTEXITCODE -ne 0) {
    throw 'Unable to read git status.'
  }
  if ($status) {
    throw "Worktree is not clean. Commit or stash changes first, or rerun with -AllowDirty."
  }
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
  $updated = [regex]::Replace($content, '(?m)^version = ".*?"$', "version = `"$NewVersion`"", 1)
  if ($updated -eq $content) {
    if ($content -notmatch ('(?m)^version = "' + [regex]::Escape($NewVersion) + '"$')) {
      throw "Failed to update Cargo version in $FilePath"
    }
    return
  }
  Write-Utf8NoBom -FilePath $FilePath -Content $updated
}

function Stop-RepoProcess {
  param(
    [string]$ExecutablePath
  )

  $fullPath = [System.IO.Path]::GetFullPath($ExecutablePath)
  $processes = Get-Process -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and [System.StringComparer]::OrdinalIgnoreCase.Equals($_.Path, $fullPath)
  }

  foreach ($process in $processes) {
    Write-Host "Stopping locked process: $($process.ProcessName) ($($process.Id))" -ForegroundColor Yellow
    Stop-Process -Id $process.Id -Force -ErrorAction Stop
  }
}

function Ensure-NsisOnPath {
  if (Get-Command makensis -ErrorAction SilentlyContinue) {
    return
  }

  $userNsisBin = Join-Path $env:LOCALAPPDATA 'NSIS'
  $userMakensis = Join-Path $userNsisBin 'makensis.exe'
  if (Test-Path $userMakensis) {
    $env:PATH = "$userNsisBin;$env:PATH"
  }
}

function Get-ReleaseNotesChangesTemplate {
  return @(
    '- TODO: Summarize the major user-facing changes in this release.',
    '- TODO: List the most important fixes or behavior changes.',
    '- TODO: Mention deployment or packaging changes if they matter to users.'
  )
}

function Get-ReleaseNotesVerification {
  return @(
    '- Ran `cargo build --release --manifest-path src-tauri/Cargo.toml --no-default-features --features web --bin acliv-web`',
    '- Ran `npm run tauri build`',
    '- Collected standard release artifacts into the `release/` directory'
  )
}

function Find-SingleFile {
  param(
    [string]$Glob
  )

  $files = Get-ChildItem $Glob -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending
  if (-not $files) {
    throw "No file matched: $Glob"
  }
  return $files[0].FullName
}

function Copy-Artifact {
  param(
    [string]$Source,
    [string]$Destination
  )

  Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

Assert-CleanWorktree

$releaseDir = Join-Path $RepoRoot "release\v$Version"
$packageJson = Join-Path $RepoRoot 'package.json'
$packageLock = Join-Path $RepoRoot 'package-lock.json'
$cargoToml = Join-Path $RepoRoot 'src-tauri\Cargo.toml'
$tauriConfig = Join-Path $RepoRoot 'src-tauri\tauri.conf.json'
$desktopTargetExe = Join-Path $RepoRoot 'src-tauri\target\release\acliv.exe'
$webTargetExe = Join-Path $RepoRoot 'src-tauri\target\release\acliv-web.exe'

Invoke-Step -Name 'sync version files' -Action {
  Set-JsonVersion -FilePath $packageJson -NewVersion $Version
  if (Test-Path $packageLock) {
    Set-PackageLockVersion -FilePath $packageLock -NewVersion $Version
  }
  Set-CargoVersion -FilePath $cargoToml -NewVersion $Version
  Set-JsonVersion -FilePath $tauriConfig -NewVersion $Version
}

Invoke-Step -Name 'clean previous build output' -Action {
  Stop-RepoProcess -ExecutablePath $desktopTargetExe
  Stop-RepoProcess -ExecutablePath $webTargetExe
  if (Test-Path 'dist') { Remove-Item -LiteralPath 'dist' -Recurse -Force }
  if (Test-Path 'src-tauri\target\release') { Remove-Item -LiteralPath 'src-tauri\target\release' -Recurse -Force }
  if (Test-Path $releaseDir) { Remove-Item -LiteralPath $releaseDir -Recurse -Force }
  New-Item -ItemType Directory -Path $releaseDir | Out-Null
}

Invoke-Step -Name 'build web binary' -Action {
  cargo build --release --manifest-path src-tauri/Cargo.toml --no-default-features --features web --bin acliv-web
}

Invoke-Step -Name 'build desktop bundle' -Action {
  Ensure-NsisOnPath
  if (-not (Get-Command makensis -ErrorAction SilentlyContinue)) {
    throw 'NSIS is required to build the setup bundle. Install NSIS and ensure makensis is available on PATH.'
  }
  npm run tauri build
}

$desktopExe = Join-Path $RepoRoot 'src-tauri\target\release\acliv.exe'
$setupExe = Find-SingleFile -Glob (Join-Path $RepoRoot 'src-tauri\target\release\bundle\nsis\*.exe')
$msiFile = Find-SingleFile -Glob (Join-Path $RepoRoot 'src-tauri\target\release\bundle\msi\*.msi')
$webBinary = Join-Path $RepoRoot 'src-tauri\target\release\acliv-web.exe'

Invoke-Step -Name 'collect release artifacts' -Action {
  Copy-Artifact -Source $desktopExe -Destination (Join-Path $releaseDir "acliv-v$Version.exe")
  Copy-Artifact -Source $setupExe -Destination (Join-Path $releaseDir "acliv-v$Version-x64-setup.exe")
  Copy-Artifact -Source $msiFile -Destination (Join-Path $releaseDir "acliv-v$Version-x64-en-us.msi")
  if (Test-Path $webBinary) {
    Copy-Artifact -Source $webBinary -Destination (Join-Path $releaseDir "acliv-web-v$Version.exe")
  }
}

$releaseNotesPath = Join-Path $releaseDir "release-notes-v$Version.md"
Invoke-Step -Name 'generate release notes template' -Action {
  $changes = (Get-ReleaseNotesChangesTemplate) -join [Environment]::NewLine
  $verification = (Get-ReleaseNotesVerification) -join [Environment]::NewLine
  $releaseNotes = @"
# v$Version

## Changes

$changes

## Verification

$verification
"@
  Write-Utf8NoBom -FilePath $releaseNotesPath -Content $releaseNotes
}

Write-Host "Release artifacts ready: $releaseDir" -ForegroundColor Green
