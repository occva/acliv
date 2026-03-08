# Repository Guidelines

## Project Structure & Module Organization
This project is a Tauri desktop app with a Svelte frontend and a Rust backend.

- `src/`: Svelte 5 + TypeScript UI (`App.svelte`, `lib/api.ts`, `lib/components/Markdown.svelte`).
- `public/`: static assets and global styles (`public/css/style.css`).
- `src-tauri/src/`: Rust application code:
- `lib.rs` registers Tauri commands and plugins.
- `cmd.rs` contains `#[tauri::command]` entry points.
- `paths.rs` resolves provider data paths.
- `session_manager/` contains provider adapters (`claude`, `codex`, `gemini`, `openclaw`, `opencode`).
- `docs/`: architecture and migration notes.

## Build, Test, and Development Commands
Run from repository root:

- `npm install`: install JS dependencies.
- `npm run dev`: run Vite frontend only.
- `cargo tauri dev`: run the full desktop app in development.
- `npm run build`: build frontend assets.
- `cargo tauri build`: build production desktop bundles.
- `npx svelte-check`: run Svelte/TypeScript diagnostics.
- `cargo test --manifest-path src-tauri/Cargo.toml`: run Rust tests.

## Coding Style & Naming Conventions
- TypeScript/Svelte: 2-space indentation, `camelCase` for functions/variables, `PascalCase` for components.
- Rust: follow `rustfmt`, use `snake_case` for modules/functions and `PascalCase` for structs/enums.
- Keep frontend API names aligned with Tauri command names (example: `invoke("list_sessions")` maps to Rust command registration).
- Prefer small, focused modules; keep provider-specific logic inside `session_manager/providers/*`.

## Testing Guidelines
- Rust tests should be colocated with modules using `#[cfg(test)]`.
- Focus coverage on parsing, path resolution, and provider normalization logic.
- Before PRs, run at least `npx svelte-check` and `cargo test --manifest-path src-tauri/Cargo.toml`.
- If UI behavior changes, include manual verification notes for session list loading, message rendering, and provider filtering.

## Commit & Pull Request Guidelines
Recent history uses Conventional Commit style, including scoped commits, often in Chinese:
- `feat(backend): ...`
- `feat(frontend): ...`
- `docs(readme): ...`

Use format: `type(scope): imperative summary` (for example: `fix(session_manager): handle empty sourcePath`).

PRs should include:
- change summary and motivation,
- linked issue/task (if any),
- validation steps run locally,
- screenshots or short recordings for UI-impacting changes.
