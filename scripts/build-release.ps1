[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Version,
  [switch]$AllowDirty,
  [ValidateSet('template', 'git')]
  [string]$ReleaseNotesMode = 'template'
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

  $candidateBins = @()
  if ($env:LOCALAPPDATA) {
    $candidateBins += @(
      (Join-Path $env:LOCALAPPDATA 'NSIS'),
      (Join-Path $env:LOCALAPPDATA 'tauri\NSIS'),
      (Join-Path $env:LOCALAPPDATA 'tauri\NSIS\Bin')
    )
  }
  if ($env:ProgramFiles) {
    $candidateBins += (Join-Path $env:ProgramFiles 'NSIS')
  }
  $programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
  if ($programFilesX86) {
    $candidateBins += (Join-Path $programFilesX86 'NSIS')
  }
  if ($env:ChocolateyInstall) {
    $candidateBins += @(
      (Join-Path $env:ChocolateyInstall 'bin'),
      (Join-Path $env:ChocolateyInstall 'lib\nsis\tools')
    )
  }
  $candidateBins += @(
    'C:\Program Files\NSIS',
    'C:\Program Files (x86)\NSIS',
    'C:\ProgramData\chocolatey\bin',
    'C:\ProgramData\chocolatey\lib\nsis\tools'
  )

  foreach ($candidateBin in $candidateBins) {
    $candidateMakensis = Join-Path $candidateBin 'makensis.exe'
    if (Test-Path $candidateMakensis) {
      $env:PATH = "$candidateBin;$env:PATH"
      return
    }
  }
}

function Get-ReleaseNotesChangesTemplate {
  return @(
    '- TODO: Summarize the major user-facing changes in this release.',
    '- TODO: List the most important fixes or behavior changes.',
    '- TODO: Mention deployment or packaging changes if they matter to users.'
  )
}

function Get-GitReleaseNotesChanges {
  $rawSubjects = git log --pretty=format:%s -n 12
  if ($LASTEXITCODE -ne 0) {
    throw 'Failed to generate release note candidates from git history.'
  }

  $subjects = @(
    $rawSubjects -split "`r?`n" |
      ForEach-Object { $_.Trim() } |
      Where-Object { $_ -and ($_ -notmatch '^Merge\b') }
  )
  if ($subjects.Count -eq 0) {
    return @("- Automated release build for v$Version.")
  }

  return $subjects | ForEach-Object { "- $_" }
}

function Get-ReleaseNotesChanges {
  param(
    [ValidateSet('template', 'git')]
    [string]$Mode
  )

  if ($Mode -eq 'git') {
    return Get-GitReleaseNotesChanges
  }

  return Get-ReleaseNotesChangesTemplate
}

function Get-ReleaseNotesVerification {
  return @(
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
$cargoLock = Join-Path $RepoRoot 'src-tauri\Cargo.lock'
$webCargoToml = Join-Path $RepoRoot 'src-tauri\web\Cargo.toml'
$webCargoLock = Join-Path $RepoRoot 'src-tauri\web\Cargo.lock'
$tauriConfig = Join-Path $RepoRoot 'src-tauri\tauri.conf.json'
$desktopTargetExe = Join-Path $RepoRoot 'src-tauri\target\release\acliv.exe'

Invoke-Step -Name 'sync version files' -Action {
  Set-JsonVersion -FilePath $packageJson -NewVersion $Version
  if (Test-Path $packageLock) {
    Set-PackageLockVersion -FilePath $packageLock -NewVersion $Version
  }
  Set-CargoVersion -FilePath $cargoToml -NewVersion $Version
  Set-CargoLockPackageVersion -FilePath $cargoLock -PackageName 'acliv' -NewVersion $Version
  Set-CargoVersion -FilePath $webCargoToml -NewVersion $Version
  Set-CargoLockPackageVersion -FilePath $webCargoLock -PackageName 'acliv-web' -NewVersion $Version
  Set-JsonVersion -FilePath $tauriConfig -NewVersion $Version
}

Invoke-Step -Name 'clean previous build output' -Action {
  Stop-RepoProcess -ExecutablePath $desktopTargetExe
  if (Test-Path 'dist') { Remove-Item -LiteralPath 'dist' -Recurse -Force }
  if (Test-Path 'src-tauri\target\release') { Remove-Item -LiteralPath 'src-tauri\target\release' -Recurse -Force }
  if (Test-Path $releaseDir) { Remove-Item -LiteralPath $releaseDir -Recurse -Force }
  New-Item -ItemType Directory -Path $releaseDir | Out-Null
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

Invoke-Step -Name 'collect release artifacts' -Action {
  Copy-Artifact -Source $desktopExe -Destination (Join-Path $releaseDir "acliv-v$Version.exe")
  Copy-Artifact -Source $setupExe -Destination (Join-Path $releaseDir "acliv-v$Version-x64-setup.exe")
  Copy-Artifact -Source $msiFile -Destination (Join-Path $releaseDir "acliv-v$Version-x64-en-us.msi")
}

$releaseNotesPath = Join-Path $releaseDir "release-notes-v$Version.md"
Invoke-Step -Name 'generate release notes' -Action {
  $changes = (Get-ReleaseNotesChanges -Mode $ReleaseNotesMode) -join [Environment]::NewLine
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
