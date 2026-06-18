<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux AI</h1>

<p align="center">
  <b>The native terminal built for AI coding agents.</b><br/>
  Run Codex, Claude Code, and 6 more AI coding CLIs in one project-aware terminal — live agent state, token analytics, durable memory, agent-safe SSH, and phone handoff.
</p>

<p align="center">
  <a href="https://codux.dux.cn">Website</a> &middot;
  <a href="https://github.com/duxweb/codux/releases/latest">Download</a> &middot;
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

AI coding CLIs are incredibly powerful — and incredibly easy to lose control of. Real work sprawls across projects, Git worktrees, terminals, sessions, tokens, remote shells, and context you half-remember. **Codux AI turns that chaos into one durable, native workspace built for serious AI coding.**

| When AI coding gets messy | Codux AI gives you |
| :------------------------ | :----------------- |
| Every AI CLI has its own state | One project-aware view across Codex, Claude Code, Gemini CLI, OpenCode, Kiro CLI, Kimi Code, CodeWhale, and Agy. |
| Long agent runs are hard to resume | Live status, local history, session restore, and context that follows each worktree. |
| Parallel tasks collide | A worktree-first model where every task keeps its own terminals, Git state, files, and AI sessions. |
| Token spend is a black box | Usage by tool, model, project, worktree, and day — no spreadsheets. |
| Context evaporates between sessions | Local memory for habits, project profiles, and module notes, injected back into supported CLIs automatically. |
| Server access is fragile | Saved, tested SSH profiles and a `codux-ssh` command agents can use **without ever seeing your credentials**. |
| You walk away mid-run | Pair your phone over Iroh and keep driving the session from anywhere. |

Codux AI is **not** another editor. It's the control plane for developers who live in AI coding CLIs and need a rock-solid way to run multi-project, long-running agent work.

## Every AI CLI, one place

Codux auto-detects the AI CLIs you run in the terminal, reads their local history, and — where the tool allows — sets up the integrations and memory files for you. Zero config.

| Tool | Live Status | History | Resume | Memory Injection |
| :--- | :------------- | :------------ | :----- | :--------------- |
| Codex | Full | Full | Full | Yes |
| Claude Code | Full | Full | Full | Yes |
| Gemini CLI | Full | Full | Tool-dependent | Yes |
| OpenCode | Full | Full | Tool-dependent | Yes |
| Kiro CLI | Full | Full | Tool-dependent | Yes |
| Kimi Code | Full | Full | Tool-dependent | Tool-dependent |
| CodeWhale | Full | Full | Tool-dependent | Yes |
| Agy | Full | Full | Tool-dependent | Yes |

`Full` means Codux tracks that capability from everyday use. `Tool-dependent` means the workspace and history are preserved while exact resume behavior is up to the CLI.

Every tool gets deep, first-class support, so sessions never cross state and adding a new one stays easy.

## Built for long agent runs

Codux isn't a terminal with tabs — it's an AI-aware control layer that keeps long-running agent work **visible, recoverable, and safe to continue**.

- **See what the agent is actually doing.** Running, completed, interrupted, permission-waiting, plan-updating — every session tied to the right project and worktree, with the task plan shown when the CLI exposes it.
- **A terminal tuned for AI.** Copy, colors, full-screen apps, key combos, mouse — everything you expect from a terminal, smooth even through long agent runs.
- **Token spend, made visible.** Usage by tool, model, project, worktree, and day — no spreadsheets.
- **Memory that follows the work, kept local.** Codux mines durable preferences, project profiles, and module notes from your sessions, filters the noise, and injects only what's relevant. History and memory never leave your machine.
- **Project surfaces beside the terminal.** Browse files, preview Markdown and images, and review Git changes in focused diff windows — all within reach.
- **Clipboard & drag-and-drop made AI-friendly.** Pasted images turn into local file paths (not a wall of base64); dropped files insert ready-to-use paths — hand them straight to the AI.
- **Let the AI reach servers, safely.** Run remote commands through saved, tested SSH profiles — your passwords and keys are never exposed to the AI.

## Phone-to-desktop handoff

Stepped away from your desk? Pair your phone with the desktop and keep driving the session from anywhere.

- Scan a QR code to pair in seconds; it picks the fastest connection automatically and falls back to a relay when needed.
- Projects, terminals, files, and AI sessions all keep running on the desktop — your phone just controls them, and you still see the full terminal history when you switch over.

## Desktop pets

Optional companions that grow with your AI coding habits — they react to usage, reminders, and agent activity. Import Codex-style custom pet packs from Petdex with a flat `pet.json` + `spritesheet.png` format.

## Worktree-first workflow

Codux models real AI work the way it actually happens: **Project → Worktree / Task → Terminals, Files, Git, AI Sessions.**

- Spin up Git worktrees for parallel tasks without tangling branch state.
- Switch tasks and keep everything — terminal tabs, splits, panel sizes, active AI sessions, file context, and Git state.
- Review worktree changes against the base branch, merge back, and clean up finished worktrees.
- Keep AI history and runtime activity scoped to the worktree, while project memory stays shared.

This is what sets Codux apart from a plain terminal multiplexer: it *knows* which project and worktree each terminal belongs to, and rebuilds the whole workspace around that relationship.

## Native, not Electron

Codux is built in **Rust + GPUI** — the same native stack behind [Zed](https://zed.dev) — so terminal rendering, project switching, and long, heavy agent runs stay fast and smooth, without the bloat and memory drain of Electron. Desktop and mobile share one terminal core for an identical experience, and it's already paving the way for future Linux hosts.

## Download

**[Download the latest release →](https://github.com/duxweb/codux/releases/latest)** &nbsp;·&nbsp; or visit [codux.dux.cn](https://codux.dux.cn)

| Platform | Installer (links to the latest version) |
| :------- | :--- |
| macOS | [`codux-*-macos.dmg`](https://github.com/duxweb/codux/releases/latest) — open and drag Codux to Applications |
| Windows | [`codux-*-windows-x86_64-setup.exe`](https://github.com/duxweb/codux/releases/latest) — double-click to install |

Then open a project, start your AI CLI in the terminal, and go — spin up a worktree for parallel tasks, connect an SSH profile, or pair your phone when you need to.

## Keyboard Shortcuts

| Action | Shortcut |
| :----- | :------- |
| New Split | `⌘T` |
| New Tab | `⌘D` |
| Toggle Git Panel | `⌘G` |
| Toggle AI Panel | `⌘Y` |
| Switch Project | `⌘1` – `⌘9` |

Customize everything in **Settings → Shortcuts**.

## Demo Video

GitHub READMEs can't embed third-party players — watch the demo on [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/).

## WeChat

Scan to add the author on WeChat and ask to join the DUXAI community group.

<p align="center">
  <img src="docs/images/wechat-author.png" width="320" alt="Author WeChat QR code">
</p>

## Repository Layout

This repo is the Codux monorepo:

- `apps/desktop` — Rust + GPUI desktop app, runtime, assets, and release scripts.
- `apps/agent` — headless controlled-agent app linking protocol, terminal core, and the shared local PTY driver without GPUI.
- `apps/mobile` — Flutter mobile controller.
- `crates/codux-protocol` — shared remote protocol: capabilities, envelope DTOs, transport candidates, and relay rules.
- `crates/codux-protocol-ffi` — Flutter-facing C ABI for the protocol and terminal-core bindings.
- `crates/codux-runtime-core` — shared runtime domain rules for host, project, file, Git, worktree, upload, and terminal shapes.
- `crates/codux-terminal-core` — shared terminal session, sequencing, baseline restore, and remote-PTY model (pure-Rust `alacritty_terminal` engine).
- `crates/codux-terminal-pty` — shared `portable_pty` local PTY driver for host/headless targets.

Flutter keeps its own native build system. Remote connectivity runs entirely on the shared Iroh transport.

## Development

```bash
cargo run
```

Useful checks before submitting changes:

```bash
cargo check
cargo test
node apps/desktop/scripts/release/test-package-gpui.mjs
```

Desktop releases are cut by pushing a version tag such as `v1.6.2`. The release workflow builds native macOS and Windows artifacts, publishes the GitHub Release, and updates the configured updater channel.

## System Requirements

- macOS 14.0 (Sonoma) or later
- Windows 11

## Feedback

Found a bug or have a feature request? Open an [issue on GitHub](https://github.com/duxweb/codux/issues).

For bug reports, use **Help → Export Diagnostics** and attach the generated `.zip` — it bundles runtime logs, rotated logs, performance summaries, saved app state, invalid-state backups, and matching macOS diagnostic reports when available.

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
  Wanted to be dmux, but that name was taken. So it's Codux now — which sounds like "Cool Dux" in Chinese.
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
