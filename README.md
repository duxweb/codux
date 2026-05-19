# Codux Tauri

Tauri v2 rewrite workspace for Codux. The first version targets a cross-platform desktop shell with a Codux-style UI, React + TypeScript + UnoCSS on the frontend, and a Rust PTY backend rendered through xterm.js.

## Stack

- Tauri v2
- React + TypeScript + Vite
- UnoCSS
- xterm.js
- Rust + portable-pty

## Development

```bash
pnpm install
pnpm tauri dev
```

Frontend-only preview:

```bash
pnpm dev
```

Build and checks:

```bash
pnpm build
cd src-tauri && cargo check && cargo fmt --check
```

## Current scope

- Codux workspace shell with project sidebar, top runtime bar, terminal area, right panels, and Monaco-ready file tab placeholders.
- Cross-platform PTY service exposed through Tauri commands.
- xterm.js terminal rendering with native Tauri event streaming.
- Remote/mobile relay and AI runtime panels are scaffolded for the existing Codux protocol, but not fully implemented yet.

Ghostty is intentionally not part of the first-version terminal path. The default terminal path is portable PTY + xterm.js for cross-platform stability.
