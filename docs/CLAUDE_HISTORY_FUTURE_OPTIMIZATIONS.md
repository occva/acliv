# Claude History Future Optimizations

This document tracks future improvements for Claude history parsing and resume UX.

## 1) Preserve message-chain fields in Claude provider

- Keep `uuid -> msg_uuid`
- Keep `parentUuid -> parent_uuid`
- Keep `isSidechain -> is_sidechain`

Why:
- Enables stricter conversation reconstruction
- Supports future branch-aware history views

## 2) Web resume command ergonomics

Current web mode behavior is clipboard-based (no terminal launch), which is correct.

Future improvement:
- If `projectDir` exists, copy `cd <projectDir> && claude --resume <sessionId>`
- Fallback to `claude --resume <sessionId>` when `projectDir` is missing

## 3) Meta-message display policy

Product recommendation:
- Show `/insights` output (user-meaningful history)
- Show command caveat as collapsed/weak-emphasis block
- Hide slash-command injected long templates by default
- Provide a toggle: `Show system/meta messages`

Rationale:
- Keep default history view clean
- Preserve full-fidelity audit view when needed
