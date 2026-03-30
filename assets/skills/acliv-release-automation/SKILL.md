---
name: acliv-release-automation
description: Run the ACLIV release workflow for this repository. Use this skill whenever the user wants to prepare, verify, rehearse, build, tag, or publish a desktop release for this Tauri project, including release requests in English or Chinese, phrases like release, publish, GitHub Release, version bump, packaging, rehearsal, 1.0.4, or run the release scripts. This skill is project-specific and should be used instead of generic release advice whenever the task targets this repository.
---

# ACLIV Release Automation

This skill is only for the repository root that contains:

- `scripts/check.ps1`
- `scripts/build-release.ps1`
- `scripts/publish-release.ps1`
- `.github/workflows/release.yml`
- `src-tauri/`

Do not use this skill for other repositories.

## Goal

Drive the real release flow end to end for this project:

1. verify release readiness
2. sync version files
3. build release artifacts
4. optionally publish the GitHub Release

Prefer executing the existing project scripts instead of reimplementing the workflow ad hoc.

## Repo Facts

- Version files that must stay aligned:
  - `package.json`
  - `package-lock.json`
  - `src-tauri/Cargo.toml`
  - `src-tauri/tauri.conf.json`
  - `src-tauri/Cargo.lock`
- Release check entrypoint:
  - `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check.ps1 -Scope release`
- Release build entrypoint:
  - `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Version <x.y.z>`
- Release publish entrypoint:
  - `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\publish-release.ps1 -Version <x.y.z>`
- Built artifacts are copied to:
  - `release/v<version>/`

## Required Behavior

Follow this sequence unless the user explicitly asks for only part of it.

### 1. Establish scope

Confirm the current repo root is this project.

Inspect at least:

- `git status --short`
- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

If the user gives a target version, use it. Otherwise infer the intended version from the conversation and existing files. Be explicit about the version you are acting on.

### 2. Run readiness checks first

Always run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check.ps1 -Scope release
```

If it fails, stop and fix the actual issue before attempting release build or publish.

### 3. Build the release artifacts

Use the project build script, not a manual sequence, unless the script itself is broken and needs repair:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Version <x.y.z>
```

If the working tree is intentionally dirty and the user is doing a rehearsal or controlled release prep, use:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Version <x.y.z> -AllowDirty
```

After build, inspect `release/v<version>/` and confirm the expected files exist.

Expected desktop artifacts:

- `acliv-v<version>.exe`
- `acliv-v<version>-x64-setup.exe`
- `acliv-v<version>-x64-en-us.msi`

Optional additional artifact:

- `acliv-web-v<version>.exe`

Expected notes file:

- `release-notes-v<version>.md`

### 4. Publish only when asked

Publishing creates tags and pushes to GitHub. Treat it as a real side effect.

Before publish, verify:

- `gh auth status`
- release artifacts exist in `release/v<version>/`
- the user asked to actually publish, not just rehearse

Then run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\publish-release.ps1 -Version <x.y.z>
```

### 5. Summarize outcomes

Report:

- version used
- checks run
- artifacts produced
- whether publish was executed or intentionally skipped
- remaining blockers, if any

## Known Project-Specific Pitfalls

These are real issues already discovered in this repo. Watch for them and do not regress them.

### Package lock version editing

Do not rely on naive `ConvertFrom-Json` round-tripping for `package-lock.json`. The repo release script already handles this safely. If you must edit version files, preserve formatting as much as practical and avoid BOM corruption.

### Locked release executables

If `src-tauri/target/release/acliv.exe` is running, cleanup can fail. Prefer the project release script, which already stops repo-local release processes before cleaning.

### Build order matters

This repo's Tauri bundle includes `acliv-web.exe`. The web binary must exist before `tauri build` runs, otherwise WiX `light.exe` can fail while packaging MSI.

### NSIS dependency

Desktop setup bundling needs `makensis`.

- In CI, `.github/workflows/release.yml` installs NSIS.
- Locally, if `makensis` is not on `PATH`, check `%LOCALAPPDATA%\NSIS\makensis.exe` and add that directory to the process `PATH` for the current run.

## Decision Rules

- If the user asks to prepare a release or validate the release pipeline, run checks and build, but do not publish unless they explicitly ask.
- If the user asks for a formal release, run checks, build if needed, then publish.
- If scripts fail, repair the scripts first and rerun from the failing stage.
- Prefer fixing root-cause automation issues over giving manual workaround steps.
- Never delete user changes or reset the repo to make release easier.

## Command Reference

### Verify

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check.ps1 -Scope release
```

### Build

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Version <x.y.z>
```

### Build with dirty tree

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Version <x.y.z> -AllowDirty
```

### Publish

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\publish-release.ps1 -Version <x.y.z>
```

## Example Prompts This Skill Should Handle

Example 1:
User: `Start validating and preparing 1.0.4, but do not publish yet`

Behavior:

- run release checks
- build `1.0.4` artifacts
- verify `release/v1.0.4/`
- stop before publish

Example 2:
User: `Publish 1.0.4 for real`

Behavior:

- verify current version is `1.0.4`
- run checks
- build if artifacts are missing or stale
- confirm `gh auth status`
- publish via `scripts/publish-release.ps1`

Example 3:
User: `release.yml and the scripts are written, run a rehearsal`

Behavior:

- treat as rehearsal, not formal publish
- run checks and build
- inspect artifacts and workflow assumptions
- fix automation gaps if found
