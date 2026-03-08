# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a **desktop application** built with **Tauri 2.0** (Rust backend) and **Svelte 5** (frontend). It reads and displays AI CLI conversation history from Claude CLI, Codex CLI, and Gemini CLI stored in the user's home directory.

**Key architecture:** WebView (Svelte) ↔ Tauri Command (IPC) ↔ Rust Core ↔ File System

## Development Commands

```bash
# Install dependencies
npm install

# Development mode (full app with frontend + Rust backend)
cargo tauri dev

# Frontend only
npm run dev

# Type checking
npx svelte-check

# Production build
cargo tauri build

# Rust tests
cargo test --manifest-path src-tauri/Cargo.toml

# Rust formatting
cargo fmt --manifest-path src-tauri/Cargo.toml

# Rust linting
cargo clippy --manifest-path src-tauri/Cargo.toml
```

## Architecture

### Backend (Rust) - `src-tauri/src/`
- **[lib.rs](src-tauri/src/lib.rs)** - Tauri app setup, command registration, plugin initialization
- **[models.rs](src-tauri/src/models.rs)** - Data structures: `Message`, `Conversation`, `ConversationSummary`, `ProjectInfo`, `Stats`, `SearchResult`
- **[loader.rs](src-tauri/src/loader.rs)** - Core data loading logic with parallel processing (Rayon), caching, and ReDoS prevention
- **[cmd.rs](src-tauri/src/cmd.rs)** - Tauri command handlers with input validation

### Frontend (Svelte 5) - `src/`
- **[App.svelte](src/App.svelte)** - Main UI component using Svelte 5 Runes (`$state`, `$derived`)
- **[lib/api.ts](src/lib/api.ts)** - Type-safe wrappers around `invoke()` calls to Tauri commands
- **[lib/components/Markdown.svelte](src/lib/components/Markdown.svelte)** - Markdown renderer with syntax highlighting (highlight.js) and XSS sanitization (DOMPurify)

### Data Flow
1. Frontend calls API function from `lib/api.ts` (e.g., `getConversations(source, project)`)
2. API function invokes Tauri command via `invoke('get_conversations', { source, project })`
3. Rust handler in `cmd.rs` validates input and calls `loader.rs`
4. `loader.rs` checks cache, loads from filesystem, returns data
5. Data serialized via Serde, passed back to frontend as JSON

## Key Patterns

### Tauri Commands
Commands are registered in [lib.rs](src-tauri/src/lib.rs#L45) and defined in [cmd.rs](src-tauri/src/cmd.rs):
- `get_stats(source)` - Get statistics for a data source
- `get_projects(source)` - List all projects
- `get_conversations(source, project)` - List conversations in a project
- `get_conversation_detail(source, project, session_id)` - Get full conversation with messages
- `search(source, query)` - Search conversations by title (regex-escaped)
- `reload_data(source)` - Clear cache and reload from disk
- `list_sources()` - Return available data sources: `["claude", "codex", "gemini"]`

### Data Source Discovery
History directories are discovered from `~/.claude`, `~/.codex`, `~/.gemini` - see [get_source_config()](src-tauri/src/loader.rs#L40). Do not hardcode paths.

### Svelte 5 Runes
The frontend uses Svelte 5's new reactivity system:
- `$state()` - Reactive state variables
- `$derived()` - Computed values
- No `stores/` directory needed - all state in components

### Styling
Global styles in `public/css/style.css`. Uses CSS variables for theming with `[data-theme="dark"]` and `[data-theme="light"]`.

## Security Considerations

### ReDoS Prevention
Search queries are regex-escaped via `regex::escape()` in [search_conversations()](src-tauri/src/loader.rs#L758) to prevent catastrophic backtracking attacks.

### XSS Defense
Frontend Markdown rendering uses `DOMPurify.sanitize()` in [Markdown.svelte](src/lib/components/Markdown.svelte) before rendering HTML.

### Input Validation
All Tauri commands in [cmd.rs](src-tauri/src/cmd.rs) validate:
- String length limits (e.g., project names max 255 chars, session IDs max 128)
- Empty/whitespace-only input rejection
- Return `Result<T, String>` with descriptive error messages

### File Size Limits
[load_jsonl()](src-tauri/src/loader.rs#L94) enforces 50MB max file size to prevent OOM on malformed files.

## Performance Features

- **Parallel file processing**: Uses Rayon's `par_iter()` for concurrent JSONL parsing
- **Global caching**: `DATA_CACHE` (Arc<RwLock<HashMap>>) stores loaded data per source
- **Read-only access pattern**: [with_loaded_data()](src-tauri/src/loader.rs#L643) avoids cloning entire data structures

## Naming Conventions

- **Rust**: `snake_case` for functions/modules, `PascalCase` for structs/enums
- **TypeScript/Svelte**: `camelCase` for variables/functions, `PascalCase` for components/types
- **Tauri command names**: Align Rust `snake_case` with TypeScript invoke strings (e.g., `get_conversations` ↔ `invoke('get_conversations')`)

## Testing

- Rust unit tests go in modules with `#[cfg(test)]` blocks
- Before PRs: run `cargo test --manifest-path src-tauri/Cargo.toml` and `npx svelte-check`
- No frontend test suite currently exists
