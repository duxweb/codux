<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux AI</h1>

<p align="center">
  <b>A Rust + GPUI native control plane for AI coding CLIs.</b><br/>
  Run Codex, Claude Code, Gemini CLI, OpenCode, Kiro CLI, Kimi Code, CodeWhale, and Agy across projects, worktrees, terminals, Git, memory, tokens, SSH, and mobile handoff.
</p>

<p align="center">
  <a href="https://codux.dux.cn">Website</a> &middot;
  <a href="https://github.com/duxweb/codux/releases">Download</a> &middot;
  <a href="https://github.com/duxweb/codux-flutter/releases">Mobile</a> &middot;
  <a href="#wechat">WeChat</a> &middot;
  <a href="https://github.com/duxweb/codux/issues">Feedback</a>
</p>

<p align="center">
  English | <a href="README.zh-CN.md">简体中文</a>
</p>

---

![Codux AI](docs/images/screenshot.png)

## Why Codux AI

AI coding CLIs are powerful, but serious work quickly spreads across projects, Git worktrees, terminals, sessions, tokens, remote shells, and half-remembered context. Codux AI turns that scattered workflow into one durable desktop workspace.

| When AI coding gets messy | Codux AI gives you |
| :------------------------ | :----------------- |
| Every AI CLI has its own state | One project-aware runtime view for Codex, Claude Code, Gemini CLI, OpenCode, Kiro CLI, Kimi Code, CodeWhale, and Agy. |
| Long sessions are hard to resume | Live AI runtime status, local history indexing, session restore, and per-worktree context. |
| Parallel tasks collide | A worktree-first task model where each task keeps its own terminal layout, Git state, files, and AI sessions. |
| Token usage is vague | Usage by tool, model, project, worktree, and day, without maintaining spreadsheets. |
| Project context gets lost | Local memory for user habits, project profiles, module notes, and app-managed context injection. |
| Server access is fragile | Saved SSH profiles, connection testing, and a `codux-ssh` command that AI tools can use without seeing credentials. |
| You leave the desk mid-run | Codux Mobile pairs with the desktop host through the v3 relay/WebRTC path so you can continue sessions remotely. |

Codux AI is not an editor replacement. It is a control plane for developers who already use AI coding CLIs heavily and need a stable way to manage multi-project, long-running AI work.

## Rust + GPUI Native Foundation

Codux AI is a native desktop app built with Rust and GPUI. GPUI comes from the same high-performance UI technology stack used by Zed, giving Codux a fast native foundation for terminal rendering, project switching, Git views, file previews, and long-running runtime updates.

- Rust owns the runtime, project state, terminal PTY management, remote protocol, settings, Git integration, SSH profile handling, and local indexing.
- GPUI powers the desktop interface, windowing, input handling, canvas rendering, native dialogs, and high-frequency UI updates.
- The terminal surface is built on `alacritty_terminal`, with Codux-specific selection, scrollback, colors, key mapping, mouse reporting, drag/drop, clipboard, and remote replica behavior around it.
- The architecture is cross-platform by design: current desktop releases target macOS and Windows, and the remote protocol is shaped so future Linux headless hosts can expose the same runtime domains without a GUI.

## Worktree-first Workflow

Codux models real AI work as **Project -> Worktree / Task -> Terminals, Files, Git, AI Sessions**.

- Create Git worktrees for parallel tasks without mixing branch state.
- Switch tasks while preserving terminal tabs, splits, bottom-panel height, active AI sessions, file context, and Git state.
- Review worktree changes, compare against the base branch, merge back to the mainline, and clean up finished worktrees.
- Keep AI history and runtime activity tied to the current worktree while project-level memory remains shared.

This is the main difference from a plain terminal multiplexer: Codux knows which project and worktree each terminal belongs to, then restores the workspace around that relationship.

## AI Tool Support

Codux detects supported CLIs from managed terminals, reads local session history where available, and installs app-managed hooks or memory files for tools that support them.

| Tool | Runtime Status | History Index | Resume | Memory Injection |
| :--- | :------------- | :------------ | :----- | :--------------- |
| Codex | Full | Full | Full | Yes |
| Claude Code | Full | Full | Full | Yes |
| Gemini CLI | Full | Full | Tool-dependent | Yes |
| OpenCode | Full | Full | Tool-dependent | Yes |
| Kiro CLI | Full | Full | Tool-dependent | Yes |
| Kimi Code | Full | Full | Tool-dependent | Tool-dependent |
| CodeWhale | Full | Full | Tool-dependent | Yes |
| Agy | Full | Full | Tool-dependent | Yes |

`Full` means Codux can track that capability from the normal terminal workflow. `Tool-dependent` means Codux can preserve the workspace and history, while the exact resume behavior still depends on the CLI itself.

## Runtime Driver Architecture

Codux is not just wrapping a shell. Each supported AI tool is represented by a runtime driver that keeps the integration path consistent:

- **Hooks** capture starts, completions, interruptions, permission waits, and model/session metadata.
- **Probes** detect current running sessions, tools, models, and accumulated usage.
- **History sources** normalize local CLI transcripts into one timeline.
- **Memory injection** gives supported CLIs project context without duplicating wrapper logic.

This keeps multiple Codex, Claude, Kimi Code, CodeWhale, or other sessions from crossing state, and makes new tool support easier to add without rewriting the whole runtime.

## AI CLI Optimizations

Codux is not just a terminal with tabs. It adds an AI-aware control layer around the terminal so long-running agent work stays visible, recoverable, and safe to continue:

- **Live agent state, not just text output**: Codux tracks running, completed, interrupted, permission-waiting, and plan-updating sessions, then ties each session back to the correct project and worktree.
- **Task plans when the CLI exposes them**: Codux can surface the current plan and completion state, so desktop pets and runtime views can show what the agent is actually working through.
- **A terminal tuned for long AI runs**: Scrollback, selection, ANSI color state, alternate-screen apps, modified key sequences, mouse reporting, and terminal scrollbars are handled in the managed terminal layer.
- **Prompt-safe clipboard and path handling**: Pasted images can become temporary files with local paths instead of base64 payloads; dragged files insert shell-quoted paths that AI tools can use immediately.
- **Project surfaces beside the terminal**: Text editing, Markdown preview, image preview, external-open fallbacks, and focused Git diff windows keep review work close to the running CLI.
- **Remote handoff without losing terminal state**: The v3.1 path uses bounded snapshots, sequence guards, chunking, progress, and subscriptions so mobile can recover large Codex or Claude histories safely.
- **SSH profiles built for agents**: `codux-ssh <profile>` lets AI CLIs run remote commands through saved, tested profiles without exposing passwords, passphrases, or private-key paths.
- **Local memory that follows the work**: Codux extracts durable user preferences, project profiles, and module notes from local transcripts, filters noisy boundaries, and injects only relevant context for the active project or worktree.

## AI History, Tokens, and Memory

Codux indexes AI session history locally and turns it into durable project context.

- See recent sessions by project and worktree.
- Track token usage by day, model, tool, project, and workspace.
- Extract user preferences, project profiles, and module notes into local memory.
- Queue memory extraction in the runtime so the UI stays responsive.
- Inject relevant context back into supported AI CLIs when launching them.

Memory and history are stored locally. Codux treats project lists and memory as the durable assets; AI history can be rebuilt from supported local CLI transcripts.

## Project Surfaces and Secure Connections

Codux keeps the terminal next to the project surfaces you need during AI work:

- Browse files, preview assets, and drag file paths into the terminal.
- Review Git changes, stage diffs, inspect history, pull, push, and handle worktree merges.
- Save SSH profiles with password or private-key credentials.
- Test SSH connectivity before saving.
- Connect from the SSH panel or let AI CLIs use the injected `codux-ssh <profile>` command.

`codux-ssh` references a saved profile by id. It does not expose saved passwords, key passphrases, or raw connection details to the AI CLI prompt.

Database and other secure connection profiles are planned. Today, database access should be handled through your existing CLI tools, SSH tunnels, or remote shell workflow.

## Mobile Handoff

Codux Mobile connects to the desktop host through the v3 remote path.

- Pair mobile with the desktop using a short-lived QR ticket.
- Use the global public relay by leaving the relay setting empty, choose the China node when needed, or configure a custom relay endpoint.
- Prefer WebRTC DataChannel when a direct path is available and fall back to WebSocket relay when P2P cannot connect.
- Keep projects, terminals, files, and AI sessions running on the desktop host while mobile controls the session remotely.
- Use one runtime protocol model across transport drivers: local, WebRTC, WebSocket relay, and future transports all feed the same project, terminal, file, Git/worktree, and AI-stat state.

Terminal input, output, file payloads, project lists, and AI stats are encrypted between Codux Desktop and Codux Mobile.

For implementation boundaries and the v3.1 protocol shape, see [Remote Protocol Architecture](docs/remote-protocol-architecture.md).

## Custom Pets

Codux includes optional desktop companions that grow with your AI coding habits. Pets can react to usage, reminders, and AI work patterns, and you can import Codex-style custom pet packages from Petdex using a flat `pet.json` + `spritesheet.png` format.

## Getting Started

1. Download Codux from [GitHub Releases](https://github.com/duxweb/codux/releases) or [codux.dux.cn](https://codux.dux.cn).
2. Install it:
   - macOS: open the `.dmg` and drag Codux to Applications.
   - Windows: run the `setup.exe` installer.
3. Open a project folder.
4. Start an AI CLI in the integrated terminal.
5. Optional: create a worktree task, connect an SSH profile, or pair Codux Mobile.

Recommended downloads:

| Platform | File |
| :------- | :--- |
| macOS | `codux-*-macos-*.dmg` |
| Windows | `codux-*-windows-x86_64-setup.exe` |

Updater archives and `latest.json` are published for automatic updates, fallback testing, and automation. Most users should download one of the two installers above.

## Keyboard Shortcuts

| Action | Shortcut |
| :----- | :------- |
| New Split | `⌘T` |
| New Tab | `⌘D` |
| Toggle Git Panel | `⌘G` |
| Toggle AI Panel | `⌘Y` |
| Switch Project | `⌘1` - `⌘9` |

All shortcuts can be customized in **Settings > Shortcuts**.

## Demo Video

GitHub README does not render third-party iframe players. Watch the demo on [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/).

## WeChat

Scan the QR code to add the author on WeChat and ask to join the DUXAI community group.

<p align="center">
  <img src="docs/images/wechat-author.png" width="320" alt="Author WeChat QR code">
</p>

## Development

```bash
cargo run
```

Useful checks before submitting changes:

```bash
cargo check
cargo test -p codux-runtime ssh::tests
node scripts/release/test-package-gpui.mjs
```

Desktop releases are created by pushing a version tag such as `v1.6.2`. The release workflow builds Rust-native macOS and Windows artifacts, publishes the GitHub Release, and updates the configured updater channel.

## System Requirements

- macOS 14.0 (Sonoma) or later
- Windows 11

## Feedback

Found a bug or have a feature request? Open an [issue on GitHub](https://github.com/duxweb/codux/issues).

For bug reports, use **Help -> Export Diagnostics** and attach the generated `.zip`. It includes runtime logs, rotated logs, performance summaries, saved app state, invalid state backups, and matching macOS diagnostic reports when available.

Manual log paths:

- `~/Library/Application Support/Codux/logs/runtime-rust.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`
- `%APPDATA%\Codux\logs\runtime-rust.log`

---

## Contributors

Thanks to everyone who has contributed code, issues, testing, and feedback to Codux.

<p align="center">
  <a href="https://github.com/duxweb/codux/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=duxweb/codux" alt="Codux contributors">
  </a>
</p>

## GitHub Star Trend

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  Wanted to be dmux, but that name was taken. So it's Codux now, which sounds like "Cool Dux" in Chinese.
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
