# ACLIV In-App Update Button Spec

## Summary

ACLIV needs an in-app update surface for both desktop and web mode.

Desktop can perform a real one-click update: users click the button, the app
checks the latest GitHub Release, downloads the matching installer through
Tauri's updater, installs it, and relaunches into the newest version.

Web cannot self-replace the running server from the browser. Web mode should
show current server version, check the latest release, and provide a clear
server-side update action or command handoff. If ACLIV later ships a managed web
service wrapper, that wrapper may expose a protected server-side update command.

This spec is based on `D:\code\cc-switch`'s desktop updater pattern and a
separate web update strategy for ACLIV's Rust web server.

## cc-switch Reference

Desktop:

- `src/lib/updater.ts` wraps `@tauri-apps/plugin-updater`, `getVersion`,
  `check`, `downloadAndInstall`, and `@tauri-apps/plugin-process` relaunch.
- `src/contexts/UpdateContext.tsx` performs a delayed startup check and stores
  dismissed versions in `localStorage`.
- `src/components/UpdateBadge.tsx` shows an available-update badge.
- `src/components/settings/AboutSection.tsx` provides the manual update button.
  Installed builds call `downloadAndInstall()` then relaunch; portable builds or
  failures fall back to opening GitHub Releases.
- `src-tauri/tauri.conf.json` enables `bundle.createUpdaterArtifacts` and points
  updater endpoints at
  `https://github.com/farion1231/cc-switch/releases/latest/download/latest.json`.
- `src-tauri/Cargo.toml` includes `tauri-plugin-updater = "2"` and
  `tauri-plugin-process = "2"`.
- `src-tauri/capabilities/default.json` includes `updater:default` and
  `process:allow-restart`.
- `.github/workflows/release.yml` builds signed updater artifacts and uploads
  `latest.json`.

Web:

- `cc-switch` is desktop-first and does not provide a comparable browser-side web
  self-updater.
- ACLIV web mode must therefore use a separate server deployment strategy, not
  Tauri updater APIs.

## User Experience

Add an update action in the app UI.

Desktop:

- Primary location: detail/header utility area or a small settings/about panel
  if one exists when implementation starts.
- Button label states:
  - idle: `Check for updates` / `检查更新`
  - checking: `Checking...` / `检查中...`
  - available: `Update to v<version>` / `更新到 v<version>`
  - downloading/installing: `Updating...` / `更新中...`
  - up to date: success toast only
  - error: error toast with fallback GitHub Release link

Click behavior:

1. If no update has been checked yet, click checks GitHub for the latest version.
2. If no newer version exists, show `Already up to date`.
3. If a newer version exists, keep the available update in memory and change the
   button to `Update to v<version>`.
4. Clicking the available-update button downloads and installs immediately.
5. After install succeeds, relaunch the app automatically.

Startup behavior:

- Do not auto-download or auto-install.
- Optional: delayed background check may set a badge, but it must not block
  first-screen loading.
- The core required behavior is user-initiated: click button -> update latest.

Failure behavior:

- If check fails, show `Check update failed`.
- If download/install fails, show `Update failed`.
- Provide a fallback action/link to
  `https://github.com/occva/acliv/releases/latest`.
- The app must remain usable after failure.

Web:

- Show `Current version`, `Latest version`, and deployment channel.
- `Check for updates` calls the GitHub Releases API or ACLIV's own latest
  version endpoint.
- If the server is outdated, show a copyable update command based on deployment
  type:
  - standalone binary: download latest web binary, replace service binary, restart
    service.
  - Docker/container: pull latest image and restart container.
  - manual/unknown: open GitHub Releases.
- Do not auto-run arbitrary shell commands from the browser in v1.
- If a future server-side updater command is added, it must be opt-in, protected
  by web auth, logged, and disabled by default unless the deployment explicitly
  allows self-update.

## Client Implementation

Desktop uses Tauri 2 plugins:

Use Tauri 2 plugins:

- Rust: `tauri-plugin-updater = "2"`
- Rust: `tauri-plugin-process = "2"`
- Frontend: `@tauri-apps/plugin-updater`

Register plugins in the desktop Tauri builder:

- `tauri_plugin_process::init()`
- `tauri_plugin_updater::Builder::new().build()`

Add capability permissions:

- `updater:default`
- `process:allow-restart`

Add `src/lib/updater.ts` with a thin wrapper around Tauri APIs:

- `getCurrentVersion(): Promise<string>`
- `checkForUpdate(options?: { timeout?: number }): Promise<UpdateCheckResult>`
- `installUpdate(update: UpdateHandle, onProgress?: ProgressHandler): Promise<void>`
- `relaunchApp(): Promise<void>`

Use a small Svelte state store or component-local state:

- `phase`: `idle | checking | available | downloading | installing | restarting | upToDate | error`
- `currentVersion`
- `availableVersion`
- `notes`
- `updateHandle`
- `error`

Do not persist dismissed versions for the first implementation. Keep v1 simple:
the button always lets the user check again.

Web uses normal HTTP APIs:

- Add `GET /api/app/version` returning current server version, runtime mode, OS,
  arch, and deployment type if known.
- Add `GET /api/app/latest-release` returning latest version, release URL, and
  update commands if the server can infer them.
- Frontend hides desktop install/relaunch actions in web mode and renders
  server-update guidance instead.
- GitHub requests should be timeout-bounded and non-blocking for first-screen
  load.

## Required Tauri Config

Desktop only.

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
        "https://github.com/occva/acliv/releases/latest/download/latest.json"
      ]
    }
  }
}
```

The public key is committed in config. The private key is only stored in GitHub
Actions secrets.

## Update Manifest

Desktop only.

The button uses Tauri updater, so GitHub Release must provide:

```text
latest.json
```

Minimum manifest shape:

```json
{
  "version": "1.0.13",
  "notes": "Release v1.0.13",
  "pub_date": "2026-05-28T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<signature>",
      "url": "https://github.com/occva/acliv/releases/download/v1.0.13/acliv-v1.0.13-x64-en-us.msi"
    },
    "darwin-x86_64": {
      "signature": "<signature>",
      "url": "https://github.com/occva/acliv/releases/download/v1.0.13/acliv-v1.0.13-macos-x64.tar.gz"
    },
    "darwin-aarch64": {
      "signature": "<signature>",
      "url": "https://github.com/occva/acliv/releases/download/v1.0.13/acliv-v1.0.13-macos-arm64.tar.gz"
    }
  }
}
```

Artifact rules:

- Windows updater artifact: MSI + `.sig`.
- macOS updater artifact: Tauri updater `.tar.gz` + `.sig`.
- DMG remains a manual installer and is not used by the update button.
- Portable exe is not used by the update button.

## Release Workflow Support

The existing release workflow must be extended so the in-app button has valid
data to consume.

Desktop:

- Build Tauri updater artifacts with signatures.
- Upload updater artifacts and `.sig` files to the same GitHub Release.
- Generate and upload `latest.json` after all platform assets are present.
- Fail CI if required updater artifacts or signatures are missing.

Secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Release notes rule remains unchanged:

- `docs/releases/v<version>.md` is the source of release notes.
- Scripts must not generate the `## Changes` section.

Web:

- Publish the `acliv-web` binary or image for the same release version.
- Include checksums for downloadable binaries.
- Document the supported update command per deployment target.
- The web update panel should link to the exact release used for the latest
  version.

## Acceptance Criteria

Windows:

- Install version N via MSI.
- Publish version N+1 with valid `latest.json`.
- Click the in-app update button.
- App checks, downloads, installs, relaunches, and reports N+1.

macOS:

- Install version N.
- Publish version N+1 with matching darwin platform entry.
- Click the in-app update button.
- App checks, downloads, installs, relaunches, and reports N+1.

Web:

- Web mode shows current server version and latest release version.
- Checking latest release does not block first-screen loading.
- Outdated web server shows copyable update guidance and release link.
- Browser UI does not call desktop Tauri updater APIs.

Failure cases:

- Missing network: check fails with toast, app remains usable.
- Missing `latest.json`: check fails with toast, fallback GitHub Release link is available.
- Missing signature or wrong signature: install fails safely.

## Implementation Order

1. Add updater/process dependencies and Tauri permissions for desktop.
2. Add updater config with public key and GitHub `latest.json` endpoint.
3. Add frontend updater wrapper.
4. Add the desktop update button and status toasts.
5. Add web version/latest-release endpoints and web-mode update panel.
6. Extend release workflow to publish signed desktop updater artifacts,
   `latest.json`, and web server artifacts.
7. Verify desktop with an old installed version updating to the next release.
8. Verify web with an older server binary/image showing latest release guidance.
