# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Desktop application built with **Tauri 2.0** (Rust backend) + **Svelte 5** (frontend). Reads and displays AI CLI conversation history from Claude, Codex, Gemini, OpenCode, and OpenClaw stored in the user's home directory.

Also ships a **web server mode** (`aichv-web`) that serves the same UI over HTTP without Tauri.

**Key architecture:** WebView (Svelte) ↔ Tauri IPC ↔ Rust commands ↔ `session_manager` ↔ File System

## Development Commands

```bash
# Install JS dependencies
npm install

# Full app (Tauri desktop)
cargo tauri dev

# Frontend only (Vite dev server)
npm run dev

# Type checking
npx svelte-check

# Production build (desktop)
cargo tauri build

# Web server mode (build frontend first, then run Rust web binary)
npm run web:start        # build + run
npm run web:build        # build + compile release binary

# Rust tests
cargo test --manifest-path src-tauri/Cargo.toml

# Rust formatting / linting
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml
```

## Architecture

### Backend (Rust) — `src-tauri/src/`

- **[lib.rs](../src-tauri/src/lib.rs)** — Tauri app setup, command registration (desktop feature-gated)
- **[cmd.rs](../src-tauri/src/cmd.rs)** — Tauri command handlers: `list_sessions`, `get_session_messages`, `delete_session`, `launch_session_terminal`, `open_in_file_explorer`
- **[paths.rs](../src-tauri/src/paths.rs)** — Provider data directory resolution; supports env overrides (`AICHV_CLAUDE_DIR`, `AICHV_CODEX_DIR`, `AICHV_GEMINI_DIR`, `AICHV_OPENCLAW_DIR`, `AICHV_OPENCODE_DIR`, `AICHV_HOME`)
- **[session_manager/mod.rs](../src-tauri/src/session_manager/mod.rs)** — Core types (`SessionMeta`, `SessionMessage`), parallel `scan_sessions()` (std threads), `load_messages()`, `delete_session()` dispatch
- **[session_manager/providers/](../src-tauri/src/session_manager/providers/)** — One module per provider: `claude.rs`, `codex.rs`, `gemini.rs`, `opencode.rs`, `openclaw.rs`, `utils.rs`
- **[bin/aichv-web.rs](../src-tauri/src/bin/aichv-web.rs)** — Web server binary (no Tauri, `--features web`)

### Frontend (Svelte 5) — `src/`

- **[App.svelte](../src/App.svelte)** — Main UI, Svelte 5 Runes (`$state`, `$derived`)
- **[lib/api.ts](../src/lib/api.ts)** — Type-safe `invoke()` wrappers for all Tauri commands
- **[lib/components/](../src/lib/components/)** — UI components including `Markdown.svelte` (marked + highlight.js + DOMPurify)

### Data Flow

1. Frontend calls `lib/api.ts` → `invoke('list_sessions')` etc.
2. `cmd.rs` handler spawns blocking task → calls `session_manager`
3. `session_manager::scan_sessions()` fans out to all provider modules in parallel threads
4. Each provider reads its directory (via `paths.rs`), parses JSONL/JSON, returns `Vec<SessionMeta>`
5. Results merged, sorted by `last_active_at` desc, serialized via Serde back to frontend

### Adding a New Provider

1. Create `src-tauri/src/session_manager/providers/{name}.rs` implementing `scan_sessions()`, `load_messages()`, `delete_session()`
2. Add `pub mod {name};` in `providers/mod.rs` (or add to `mod.rs` imports)
3. Add a thread in `session_manager::scan_sessions()` and dispatch arms in `load_messages()` / `delete_session()`
4. Add path resolver in `paths.rs` if needed

## Key Patterns

### Feature Flags

Desktop-only code is gated with `#[cfg(feature = "desktop")]`. The web binary uses `--no-default-features --features web`. Don't add Tauri API calls outside the `desktop` feature gate.

### Path Resolution (`paths.rs`)

All provider directories go through `resolve_provider_dir()`. Env vars take precedence over `~/.{tool}` defaults. Use `AICHV_HOME` to override home dir in tests.

### Delete Safety

`session_manager::delete_session()` canonicalizes both the provider root and the session source path, then asserts `source.starts_with(root)` before delegating to the provider. This path-traversal check must be preserved in any refactor.

### Styling

Global styles in `public/css/style.css`. Theme via CSS variables; dark/light toggled with `[data-theme="dark"]` / `[data-theme="light"]` on `<html>`.

## Security

- **Path traversal**: `delete_session_with_root()` in `session_manager/mod.rs` enforces source path is inside provider root
- **XSS**: `Markdown.svelte` runs `DOMPurify.sanitize()` before injecting HTML
- **Terminal launch**: `launch_session_terminal` is Windows-only; non-Windows returns `Err` and frontend falls back to clipboard copy

## Testing

- Rust unit tests use `#[cfg(test)]` blocks; `session_manager/mod.rs` has path-traversal tests using `tempfile`
- No frontend test suite currently
- Before PRs: `cargo test --manifest-path src-tauri/Cargo.toml` + `npx svelte-check`
