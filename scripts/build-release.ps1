[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Version,
  [switch]$AllowDirty,
  [string]$ReleaseNotesPath
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

function Ensure-TauriSigningKey {
  if ($env:TAURI_SIGNING_PRIVATE_KEY) {
    return
  }

  if ($env:GITHUB_ACTIONS -eq 'true') {
    throw 'TAURI_SIGNING_PRIVATE_KEY GitHub secret is required for CI updater artifacts.'
  }

  if ($env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    if (-not (Test-Path -LiteralPath $env:TAURI_SIGNING_PRIVATE_KEY_PATH)) {
      throw "Tauri updater signing key path not found: $env:TAURI_SIGNING_PRIVATE_KEY_PATH"
    }
    $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -LiteralPath $env:TAURI_SIGNING_PRIVATE_KEY_PATH -Raw
    Write-Host "Using local Tauri updater signing key: $env:TAURI_SIGNING_PRIVATE_KEY_PATH" -ForegroundColor Yellow
    return
  }

  $localKey = Join-Path $HOME '.acliv\tauri-updater.key'
  if (Test-Path -LiteralPath $localKey) {
    $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -LiteralPath $localKey -Raw
    Write-Host "Using local Tauri updater signing key: $localKey" -ForegroundColor Yellow
    return
  }

  throw 'Tauri updater signing key is required. Set TAURI_SIGNING_PRIVATE_KEY, or set TAURI_SIGNING_PRIVATE_KEY_PATH for local builds.'
}

function Resolve-ReleaseNotesSource {
  if ($ReleaseNotesPath) {
    return $ReleaseNotesPath
  }

  return Join-Path $RepoRoot "docs\releases\v$Version.md"
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
$desktopTargetExe = Join-Path $RepoRoot 'src-tauri\target\release\acliv.exe'
$releaseNotesSource = Resolve-ReleaseNotesSource

Invoke-Step -Name 'sync version files' -Action {
  & (Join-Path $PSScriptRoot 'sync-version.ps1') -Version $Version
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
  Ensure-TauriSigningKey
  if (-not (Get-Command makensis -ErrorAction SilentlyContinue)) {
    throw 'NSIS is required to build the setup bundle. Install NSIS and ensure makensis is available on PATH.'
  }
  npm run tauri build
}

$desktopExe = Join-Path $RepoRoot 'src-tauri\target\release\acliv.exe'
$setupExe = Find-SingleFile -Glob (Join-Path $RepoRoot 'src-tauri\target\release\bundle\nsis\*.exe')
$msiFile = Find-SingleFile -Glob (Join-Path $RepoRoot 'src-tauri\target\release\bundle\msi\*.msi')
$msiSigFile = "$msiFile.sig"

Invoke-Step -Name 'collect release artifacts' -Action {
  if (-not (Test-Path -LiteralPath $msiSigFile)) {
    throw "Missing updater signature: $msiSigFile"
  }
  Copy-Artifact -Source $desktopExe -Destination (Join-Path $releaseDir "acliv-v$Version.exe")
  Copy-Artifact -Source $setupExe -Destination (Join-Path $releaseDir "acliv-v$Version-x64-setup.exe")
  Copy-Artifact -Source $msiFile -Destination (Join-Path $releaseDir "acliv-v$Version-x64-en-us.msi")
  Copy-Artifact -Source $msiSigFile -Destination (Join-Path $releaseDir "acliv-v$Version-x64-en-us.msi.sig")
}

$releaseNotesPath = Join-Path $releaseDir "release-notes-v$Version.md"
Invoke-Step -Name 'copy release notes' -Action {
  if (-not (Test-Path -LiteralPath $releaseNotesSource)) {
    throw "Release notes source not found: $releaseNotesSource"
  }
  Copy-Artifact -Source $releaseNotesSource -Destination $releaseNotesPath
}

Write-Host "Release artifacts ready: $releaseDir" -ForegroundColor Green
