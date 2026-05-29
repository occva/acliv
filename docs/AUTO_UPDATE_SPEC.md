# In-App Update Button Spec

## Purpose

This document describes a reusable design for adding an in-app update button to
an application that has both desktop and web/runtime deployments.

The goal is to provide one clear user action:

```text
Check for updates -> show result -> install or show deployment guidance
```

Desktop builds can usually perform a real one-click update. Web deployments
usually cannot replace the running server from the browser, so the web UI should
show version status and provide safe server-side update guidance instead.

## Core Principles

- Keep update checks user-initiated unless a background check is cheap and
  delayed.
- Never block first-screen loading on release or registry network requests.
- Desktop auto-update must use signed artifacts.
- Web UI must not execute arbitrary shell commands from the browser.
- If the current runtime cannot be compared with the latest release precisely,
  show an "unknown" state instead of claiming it is current.
- Always provide a manual fallback link to the release page or deployment docs.

## Runtime Strategy

### Desktop

Desktop can use a native updater flow:

1. Read the current app version.
2. Check a signed update manifest from a release endpoint.
3. If a newer version exists, keep the update handle in memory.
4. On the second click, download and install the update.
5. Relaunch the app after installation succeeds.

Recommended state model:

```text
idle
checking
available
updating
restarting
current
unknown
error
```

### Web Or Server Runtime

Browser code should only request update information from the server. It should
not directly run system commands.

The server can expose:

- Current server version.
- Runtime type.
- Platform and architecture.
- Deployment channel.
- Current image/tag or build ref, if available.
- Latest release or latest image metadata.
- A copyable update command or operational guidance.

Container deployments can compare the running image metadata with the latest
published image metadata. A common low-friction strategy is:

1. Inject a build ref into the image at build time.
2. Expose the current build ref through `/api/app/version`.
3. Query the registry manifest for the latest image.
4. Read the latest image config labels or env metadata.
5. Compare current build ref with latest build ref.

If either side lacks comparable metadata, return `updateAvailable: null` and
show "latest status unknown" with manual update guidance.

## User Experience

Place the update action in an About, Settings, or compact utility area. Avoid
making update UI part of the first-screen critical path.

Button labels:

- `Check for updates`
- `Checking...`
- `Update to v<version>`
- `Updating...`
- `Restarting...`

Result behavior:

- No update: show a success toast such as `Already up to date`.
- Update available: keep the button visible and change it to an install action.
- Unknown web comparison: show the latest check result and update guidance.
- Check failure: show an error toast and fallback release link.
- Install failure: keep the app usable and show a manual download link.

## Desktop Implementation

For a Tauri 2 desktop app, use the updater and process plugins.

Rust dependencies:

```toml
tauri-plugin-updater = "2"
tauri-plugin-process = "2"
```

Frontend dependencies:

```json
{
  "@tauri-apps/plugin-updater": "^2",
  "@tauri-apps/plugin-process": "^2"
}
```

Register plugins in the desktop builder:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_process::init())
    .plugin(tauri_plugin_updater::Builder::new().build());
```

Add permissions:

```json
{
  "permissions": [
    "updater:default",
    "process:allow-restart"
  ]
}
```

Use a small frontend wrapper rather than spreading updater calls throughout the
UI:

```ts
export async function checkForUpdate() {
  const { check } = await import('@tauri-apps/plugin-updater');
  return await check({ timeout: 30000 });
}

export async function relaunchApp() {
  const { relaunch } = await import('@tauri-apps/plugin-process');
  await relaunch();
}
```

## Web API Shape

Suggested endpoints:

```text
GET /api/app/version
GET /api/app/update-check
```

`/api/app/version` should return:

```json
{
  "version": "1.2.3",
  "runtime": "web",
  "platform": "linux",
  "arch": "x86_64",
  "updateChannel": "container-image",
  "image": "<registry>/<namespace>/<name>",
  "imageTag": "latest",
  "buildRef": "<git-sha-or-build-id>"
}
```

`/api/app/update-check` should return:

```json
{
  "runtime": "web",
  "updateChannel": "container-image",
  "currentVersion": "1.2.3",
  "currentTag": "latest",
  "currentBuildRef": "<git-sha-or-build-id>",
  "latestTag": "latest",
  "latestDigest": "<registry-digest>",
  "latestBuildRef": "<git-sha-or-build-id>",
  "updateAvailable": true,
  "releaseUrl": "https://example.com/releases/latest",
  "updateCommand": "docker pull <image>:latest && docker compose up -d"
}
```

Use `null` for `updateAvailable` when comparison is not reliable.

## Tauri Config

Enable updater artifacts:

```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  }
}
```

Configure updater endpoint:

```json
{
  "plugins": {
    "updater": {
      "pubkey": "<TAURI_UPDATER_PUBLIC_KEY>",
      "endpoints": [
        "https://<host>/<owner>/<repo>/releases/latest/download/latest.json"
      ]
    }
  }
}
```

Commit the public key. Store the private signing key only in local secrets or CI
secrets.

## Update Manifest

Desktop updater manifests should be generated after all platform artifacts and
signatures have been uploaded.

Minimum shape:

```json
{
  "version": "1.2.3",
  "notes": "Release v1.2.3",
  "pub_date": "2026-01-01T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<signature>",
      "url": "https://<host>/download/app-v1.2.3-x64.msi"
    },
    "darwin-x86_64": {
      "signature": "<signature>",
      "url": "https://<host>/download/app-v1.2.3-macos-x64.tar.gz"
    },
    "darwin-aarch64": {
      "signature": "<signature>",
      "url": "https://<host>/download/app-v1.2.3-macos-arm64.tar.gz"
    }
  }
}
```

Artifact rules:

- Windows updater artifact: MSI plus `.sig`.
- macOS updater artifact: updater `.tar.gz` plus `.sig`.
- DMG can remain a manual installer and does not need to be used by the update
  button.
- Portable executables should be treated as manual downloads unless explicitly
  supported by the updater.

## Release Workflow

Desktop release jobs should:

1. Sync version files from the release tag.
2. Build signed updater artifacts.
3. Fail if any required artifact or `.sig` file is missing.
4. Upload all platform artifacts to the same release.
5. Generate the update manifest after all platform jobs finish.
6. Upload the manifest to the release.

Recommended CI secrets:

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Container release jobs should:

1. Build the server image.
2. Inject image tag and build ref as build args or labels.
3. Publish immutable version tags.
4. Optionally publish `latest`.
5. Ensure the running server exposes enough metadata for comparison.

For manual backfill releases, checkout the requested tag before building. This
prevents publishing an old version tag with new source code.

## Acceptance Criteria

Desktop:

- Version N is installed locally.
- Version N+1 is published with a valid manifest.
- The update button detects N+1.
- The second click downloads, installs, and relaunches.
- The app reports version N+1 after relaunch.

Web or server runtime:

- The UI shows current runtime version and deployment channel.
- Update checks do not block first-screen loading.
- Outdated deployments show copyable operational guidance.
- Unknown comparison state is shown honestly.
- Browser code does not call desktop updater APIs.

Failure cases:

- Network failure shows an error without breaking the app.
- Missing manifest shows an error and fallback release link.
- Missing or invalid signature prevents installation.
- Missing registry metadata returns unknown status, not false "current" status.

## Implementation Order

1. Add desktop updater dependencies and permissions.
2. Add updater config with public key and manifest endpoint.
3. Add a small frontend updater wrapper.
4. Add update button state and toasts.
5. Add server version and update-check endpoints.
6. Add web/runtime update guidance UI.
7. Extend release workflow to publish signed desktop updater artifacts.
8. Generate the update manifest only after all platform artifacts exist.
9. Add container metadata if web/server deployments need comparison.
10. Verify both a successful desktop update and all failure states.
