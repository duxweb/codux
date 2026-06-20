<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux AI</h1>

<p align="center">
  <b>The high-performance, cross-device terminal built for AI coding.</b><br/>
  A native <b>Rust + GPUI</b> workspace for Codex, Claude Code, and 6 more AI coding CLIs — live agent state, token analytics, durable memory, agent-safe SSH, and a desktop ⇄ phone ⇄ headless-host link that lets you drive long agent runs from anywhere.
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
| The code lives on another machine | Connect a headless host — a server, spare Mac, or Linux box — and drive its terminals, Git, and AI as if they were local. |

Codux AI is **not** another editor. It's the control plane for developers who live in AI coding CLIs and need a rock-solid way to run multi-project, long-running agent work.

## 📊 Watch & measure every AI CLI

Codux auto-detects the AI CLIs you run in the terminal, reads their local history, and — where the tool allows — sets up the integrations and memory files for you. Zero config.

- **See what the agent is doing.** Running, completed, interrupted, permission-waiting, plan-updating — every session tied to the right project and worktree, with the task plan shown when the CLI exposes it.
- **Token spend, made visible.** Usage and cost by tool, model, project, worktree, and day — no spreadsheets.

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

`Full` means Codux tracks that capability from everyday use. `Tool-dependent` means the workspace and history are preserved while exact resume behavior is up to the CLI. Every tool gets deep, first-class support, so sessions never cross state and adding a new one stays easy.

## 🔗 One workspace, every device

> **Beta.** Connecting to a headless host ships first as a beta in this release — the connection, pairing, and host-side data flow are still under active testing, so expect rough edges. Feedback is very welcome.

Desktop, phone, and a headless host all act as **peers** over an end-to-end encrypted Iroh link, so you can keep driving long agent runs from anywhere.

- **Phone handoff.** Pair in seconds by QR; it picks the fastest path and falls back to a relay. Projects, terminals, files, and AI keep running on the host — your phone just controls them, with full terminal history when you switch over.
- **Headless hosts.** Run the `codux` agent on a server, spare Mac, or Linux box and drive its terminals, Git, and AI as if local — everything runs against the host's own data. See [`apps/agent/README.md`](apps/agent/README.md).
- **Resilient sessions.** A client that drops and reconnects resumes the *same* terminals, shells, and running AI; credentials never leave the host.

## 🧠 Durable local memory

Codux mines durable preferences, project profiles, and module notes from your sessions, filters the noise, and injects only what's relevant back into supported CLIs — so context survives across sessions. History and memory never leave your machine.

## 🌳 Worktree & split-pane workflow

Codux models real AI work the way it actually happens: **Project → Worktree / Task → Terminals, Files, Git, AI Sessions.**

- Spin up Git worktrees for parallel tasks without tangling branch state.
- Split and tab terminals freely — then switch tasks and every split, panel size, active AI session, file context, and Git state comes back exactly as you left it.
- AI history and runtime activity stay scoped to the worktree, while project memory stays shared.

This is what sets Codux apart from a plain terminal multiplexer: it *knows* which project and worktree each terminal belongs to, and rebuilds the whole workspace around that relationship.

## 🔀 Git, in focused diffs

Review worktree changes against the base branch in dedicated diff windows, merge back, and clean up finished worktrees — without leaving the terminal.

## 📁 Files beside the terminal

- Browse the project tree and preview Markdown and images in focused windows.
- **Clipboard & drag-and-drop made AI-friendly.** Pasted images become local file paths (not a wall of base64); dropped files insert ready-to-use paths — hand them straight to the AI.

## 🐾 Desktop pets

Optional companions that grow with your AI coding habits — they react to usage, reminders, and agent activity. Import Codex-style custom pet packs from Petdex with a flat `pet.json` + `spritesheet.png` format.

## 🔒 Agent-safe SSH

Let the AI reach servers without ever seeing your secrets. Run remote commands through saved, tested SSH profiles and a `codux-ssh` command — your passwords and keys are never exposed to the AI.

## ⚡ Native, not Electron

Codux is built in **Rust + GPUI** — the same native stack behind [Zed](https://zed.dev) — so terminal rendering, project switching, and long, heavy agent runs stay fast and smooth, without the bloat and memory drain of Electron. Desktop, mobile, and the headless host share **one Rust terminal core**, so every device renders the same sessions identically — across macOS, Windows, and Linux.

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

Desktop releases are cut by pushing a version tag such as `v2.0.0`. The release workflow builds native macOS and Windows artifacts, publishes the GitHub Release, and updates the configured updater channel.

## System Requirements

**Desktop app**

- macOS 14.0 (Sonoma) or later
- Windows 11

**Headless host (`codux-agent`)**

- macOS, Linux, and Windows (x86_64 and arm64)

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
