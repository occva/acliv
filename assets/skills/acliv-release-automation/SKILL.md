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

Drive the real release flow for this project without drifting away from the repo's actual scripts and conventions.

For a real release request, the default sequence is:

1. determine the target version
2. generate a bilingual user-facing changelog from git history since the last `v*` release tag
3. ask the user to confirm that changelog
4. run release checks
5. build release artifacts
6. write validated release notes
7. optionally publish the GitHub Release

Prefer executing the existing project scripts instead of reimplementing the workflow ad hoc.

## Repo Facts

- Version files that must stay aligned:
  - `package.json`
  - `package-lock.json`
  - `src-tauri/Cargo.toml`
  - `src-tauri/tauri.conf.json`
- Release tag format:
  - `v<version>`
- Release title format:
  - `v<version>`
- Release notes file:
  - `release/v<version>/release-notes-v<version>.md`
- Release check entrypoint:
  - `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check.ps1 -Scope release`
- Release build entrypoint:
  - `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Version <x.y.z>`
  - CI auto notes mode: `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Version <x.y.z> -ReleaseNotesMode git`
- Release notes validation entrypoint:
  - `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\assert-release-notes.ps1 -Path release/v<version>/release-notes-v<version>.md`
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

For real publish requests, if `git status --short` is not clean, stop and ask the user how they want to proceed. Do not silently publish from a dirty worktree.

### 2. Generate the changelog before any real publish

Look up the most recent formal release tag using only tags that match `v*`.

Use git history from that tag to `HEAD` to draft a user-facing changelog:

- summarize what users will notice
- merge small bug fixes and UI tweaks into broader bullets
- keep the total bullet count at 8 or fewer
- avoid file names, internal refactors, and raw technical jargon
- do not paste commit subjects directly as final release notes

Use this exact structure when presenting the draft to the user:

```markdown
## Changes

更新内容:

- xxx
- xxx

Updates:

- xxx
- xxx
```

If the user is asking for a real release, get explicit confirmation on the changelog before publishing. If the user only wants a rehearsal or build verification, you can continue without publish, but the final publishable release notes still need this rewrite.

Useful commands:

```powershell
git tag --sort=-creatordate
git log <last_tag>..HEAD --pretty=format:%s
```

### 3. Run readiness checks

Always run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check.ps1 -Scope release
```

If it fails, stop and fix the actual issue before attempting release build or publish.

### 4. Build the release artifacts

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

Expected notes file:

- `release-notes-v<version>.md`

Before any real publish, review the notes file and make sure `## Changes` is a user-facing summary of the release:

- summarize actual product or workflow changes
- group related changes into a few clear bullets
- use the confirmed bilingual changelog
- do not paste raw commit subjects as the release notes body
- do not leave placeholder `TODO` text in a published release
- keep verification facts concrete and accurate
- keep the `## Verification` section intact unless it is factually wrong

Run the repo validator before any publish attempt:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\assert-release-notes.ps1 -Path release/v<version>/release-notes-v<version>.md
```

If the validator fails, rewrite the notes before publish.

For CI tag releases, `build-release.ps1 -ReleaseNotesMode git` can synthesize a publishable `## Changes` section from recent commit subjects before the validator runs.

### 5. Publish only when asked

Publishing creates tags and pushes to GitHub. Treat it as a real side effect.

Before publish, verify:

- `gh auth status`
- release artifacts exist in `release/v<version>/`
- `release-notes-v<version>.md` has been rewritten into a real bilingual summary and is not just commit history or TODO placeholders
- the user explicitly approved the changelog and explicitly asked to publish
- the user asked to actually publish, not just rehearse

Then run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\publish-release.ps1 -Version <x.y.z>
```

### 6. Summarize outcomes

Report:

- version used
- changelog range used, for example `v1.0.7..HEAD`
- checks run
- artifacts produced
- whether release notes were reviewed, rewritten, and validated
- whether publish was executed or intentionally skipped
- remaining blockers, if any

## Known Project-Specific Pitfalls

These are real issues already discovered in this repo. Watch for them and do not regress them.

### Package lock version editing

Do not rely on naive `ConvertFrom-Json` round-tripping for `package-lock.json`. The repo release script already handles this safely. If you must edit version files, preserve formatting as much as practical and avoid BOM corruption.

### Locked release executables

If `src-tauri/target/release/acliv.exe` is running, cleanup can fail. Prefer the project release script, which already stops repo-local release processes before cleaning.

### Desktop and Web artifacts are separate

The Tauri desktop bundle must not include `acliv-web`. Build Web artifacts separately with `src-tauri/web/Cargo.toml`.

### Local desktop rebuilds are not cargo builds

If the user only wants to rebuild and launch the desktop app locally, use:

```powershell
npm exec tauri build -- --no-bundle
```

Do not substitute a bare `cargo build` for desktop verification.

### NSIS dependency

Desktop setup bundling needs `makensis`.

- In CI, `.github/workflows/release.yml` installs NSIS.
- Locally, if `makensis` is not on `PATH`, check `%LOCALAPPDATA%\NSIS\makensis.exe` and add that directory to the process `PATH` for the current run.

## Decision Rules

- If the user asks to prepare a release or validate the release pipeline, run checks and build, but do not publish unless they explicitly ask.
- If the user asks for a formal release, draft the bilingual changelog first, get user confirmation, then run checks, build if needed, and publish.
- If the user only asks to rebuild or launch the desktop executable locally, prefer `npm exec tauri build -- --no-bundle` over the release packaging flow.
- When preparing release notes, prefer concise user-facing summaries over commit-by-commit narration.
- For real publish requests, treat a dirty worktree as a stop-and-confirm condition rather than something to ignore.
- Never publish if the release notes validator still finds `TODO` placeholders.
- Never publish if the user has not yet confirmed the changelog.
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

### Validate release notes

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\assert-release-notes.ps1 -Path release/v<version>/release-notes-v<version>.md
```

### Publish

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\publish-release.ps1 -Version <x.y.z>
```

## Example Prompts This Skill Should Handle

Example 1:
User: `Start validating and preparing 1.0.4, but do not publish yet`

Behavior:

- inspect repo status and version files
- run release checks
- build `1.0.4` artifacts
- verify `release/v1.0.4/`
- stop before publish

Example 2:
User: `根据最近提交整理 1.0.4 的更新日志，确认后再发 release`

Behavior:

- find the previous `v*` tag
- draft a bilingual changelog under 8 bullets
- wait for the user's confirmation
- do not publish yet

Example 3:
User: `Publish 1.0.4 for real`

Behavior:

- verify current version is `1.0.4`
- make sure the changelog has been confirmed by the user
- run checks
- build if artifacts are missing or stale
- rewrite `release-notes-v1.0.4.md` into a concise bilingual summary if needed
- validate notes
- confirm `gh auth status`
- publish via `scripts/publish-release.ps1`

Example 4:
User: `release.yml and the scripts are written, run a rehearsal`

Behavior:

- treat as rehearsal, not formal publish
- run checks and build
- inspect artifacts and workflow assumptions
- fix automation gaps if found
