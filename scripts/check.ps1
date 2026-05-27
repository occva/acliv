[CmdletBinding()]
param(
  [ValidateSet('local', 'push', 'release')]
  [string]$Scope = 'local'
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

Invoke-Step -Name 'svelte-check' -Action {
  npx svelte-check
}

Invoke-Step -Name 'cargo check (desktop)' -Action {
  cargo check --manifest-path src-tauri/Cargo.toml
}

if ($Scope -in @('push', 'release')) {
  Invoke-Step -Name 'cargo test' -Action {
    cargo test --manifest-path src-tauri/Cargo.toml
  }
}

if ($Scope -eq 'release') {
  Invoke-Step -Name 'cargo check (web)' -Action {
    cargo check --manifest-path src-tauri/web/Cargo.toml
  }
}

Write-Host "Check complete: $Scope" -ForegroundColor Green
