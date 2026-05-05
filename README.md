<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux</h1>

<p align="center">
  Your macOS workstation for AI coding.<br/>
  Native SwiftUI + AppKit · GPU-accelerated terminal · built for <b>Claude Code</b>, <b>Codex</b>, <b>Gemini CLI</b>, and <b>OpenCode</b>.
</p>

<p align="center">
  <a href="https://codux.dux.cn">Website</a> &middot;
  <a href="https://github.com/duxweb/codux/releases">Download</a> &middot;
  <a href="https://github.com/duxweb/codux-flutter/releases">Mobile</a> &middot;
  <a href="https://github.com/duxweb/codux-service/releases">Relay Service</a> &middot;
  <a href="https://github.com/duxweb/codux/issues">Feedback</a>
</p>

<p align="center">
  English | <a href="README.zh-CN.md">简体中文</a>
</p>

---

![Codux](docs/images/screenshot.png)

## Demo Video

GitHub README does not render third-party iframe players. Watch the demo on [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/).

## 10 Highlights

| # | Feature | One-liner |
|:--|:--|:--|
| 1 | **Live AI Activity** | Real-time status + system notifications for every AI terminal — Claude / Codex / Gemini / OpenCode. |
| 2 | **AI Stats & Session Restore** | Token history per tool / model / project, plus one-click resume of any past AI session. |
| 3 | **Daily Level** | A daily ladder powered by real token usage — see exactly what you shipped today. |
| 4 | **Pet Companion** | Your AI coding buddy. Grows with your style, has its own roadmap, and chimes in once in a while. |
| 5 | **Built-in Git** | Human-friendly branches, commits, push/pull and sync — everyday Git without leaving the workspace. |
| 6 | **Project File Browser** | Per-project file manager — edit, preview, and drag files straight into the terminal. |
| 7 | **Multi-Project Workspaces** | Up to **6 split terminals** per project plus **unlimited tabs**, isolated state per project. |
| 8 | **Three-Layer AI Memory** | User / project / tool memory shared across Codex, Claude, Gemini, and OpenCode. |
| 9 | **Mobile Handoff** | Continue your AI CLI work from your phone when you step away from the Mac. |
| 10 | **Ghostty Engine & Themes** | GPU-accelerated terminal via `ghostty`, with rich light/dark theme variants. |

### 1. Live AI Activity

Each AI terminal reports its real state — thinking, waiting on input, finished, errored — surfaced in three places: the tab indicator, the project tile, and a system notification when a turn completes. You stop watching the cursor blink; Codux taps you on the shoulder.

### 2. AI Stats & Session Restore

The AI panel turns scattered runs into a usable history: token totals split by tool / model / project, daily and trend views, and **one-click resume** of any past session back into the original tool (Claude Code, Codex, Gemini CLI, OpenCode).

![Codux AI Stats](docs/images/ai-stats.png)

### 3. Daily Level

A lightweight daily ladder driven by real AI activity. Instead of raw token counts, you get a single "today" snapshot — what you ran, how much, and how today compares to a normal day.

![Codux Daily Level](docs/images/level.png)

### 4. Pet Companion

An optional pet that lives in the title bar. Different coding styles unlock different growth values and roadmaps, and the pet drops the occasional comment so a long AI session doesn't feel quite so lonely. Fully optional, easy to mute.

![Codux Pet](docs/images/pet.png)

### 5. Built-in Git

A first-class Git panel — not an embedded webview. Branch checkout / create / rename / delete, staging with line-level diffs, full commit history, and push / pull / sync with sane defaults and clear conflict resolution.

![Codux Git Panel](docs/images/git.png)

### 6. Project File Browser

A native file manager scoped to each project. Edit code inline, preview images and other assets, and drag any file straight into the terminal so your AI tool gets the right path on the first try.

### 7. Multi-Project Workspaces

Every project is its own room. Up to **6 split terminals** for parallel work, and **unlimited tabs** when 6 is not enough. Each project keeps its own layout, sessions, AI tool selection, and state across restarts.

### 8. Three-Layer AI Memory

Codux extracts long-term memory from completed AI sessions and stores it locally in `memory.sqlite3`, layered so the right context shows up at the right time:

- **User layer** — durable preferences across projects.
- **Project layer** — conventions, decisions, and lessons specific to this repo.
- **Tool layer** — app-private launch context (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`) generated for Codex, Claude Code, Gemini CLI, and OpenCode.

So Codex / Claude / Gemini / OpenCode no longer forget what you did last session. Memory files are managed by Codux and never written into your repo — your repository stays the source of truth.

### 9. Mobile Handoff

Step away from the Mac and keep going on your phone. Codux Mobile pairs with the Mac host and lets you open new AI CLI sessions, drive existing ones, browse project files, and upload images — all running on the Mac while you watch from anywhere.

| Component | Purpose | Download |
|:--|:--|:--|
| Codux Mobile | Android client: pair with the Mac, run AI CLI sessions remotely, browse files, upload images. | [Mobile Releases](https://github.com/duxweb/codux-flutter/releases) |
| Codux Service | Lightweight Go relay for device pairing and encrypted WebSocket forwarding. | [Service Releases](https://github.com/duxweb/codux-service/releases) |

For a quick trial, use one of the official trial relays in **Settings > Remote**:

| Node | URL |
|:--|:--|
| China relay direct | `https://codux-service.dux.plus` |
| Global transit acceleration | `https://codux-node.dux.plus` |

Terminal input, output, file payloads, project lists, and AI stats are end-to-end encrypted between Codux for macOS and Codux Mobile. The relay sees only routing metadata (host ID, device ID, pairing state, online state) — never decrypted terminal content. For long-term use, self-hosting `codux-service` is recommended.

### 10. Ghostty Engine & Themes

Codux embeds the [`ghostty`](https://ghostty.org) terminal engine for GPU-accelerated rendering, so even busy AI output stays smooth. Pair that with a curated set of light and dark themes that match macOS appearance changes — the workspace looks good and stays fast.

## Getting Started

### Install with Homebrew

```bash
brew install --cask duxweb/tap/codux
```

### Update with Homebrew

```bash
brew update
brew upgrade --cask codux
```

### Install from Release

1. Download the latest release from [GitHub Releases](https://github.com/duxweb/codux/releases) or [codux.dux.cn](https://codux.dux.cn)
2. Drag Codux to your Applications folder
3. Open Codux, click **New Project**, and pick a directory
4. Start typing — you're ready to go

> **"Cannot be opened because the developer cannot be verified"**
>
> Since Codux is not yet notarized by Apple, macOS may block the first launch. To fix this:
>
> ```bash
> sudo xattr -rd com.apple.quarantine /Applications/Codux.app
> ```
>
> Or go to **System Settings > Privacy & Security**, scroll down and click **Open Anyway** next to the Codux warning.

## Keyboard Shortcuts

| Action | Shortcut |
|:--|:--|
| New Split | `⌘T` |
| New Tab | `⌘D` |
| Toggle Git Panel | `⌘G` |
| Toggle AI Panel | `⌘Y` |
| Switch Project | `⌘1` - `⌘9` |

All shortcuts can be customized in **Settings > Shortcuts**.

## System Requirements

- macOS 14.0 (Sonoma) or later

## Feedback

Found a bug or have a feature request? Open an [issue on GitHub](https://github.com/duxweb/codux/issues).

When reporting a bug, the easiest path is `Help -> Export Diagnostics…` — save the generated `.zip` and attach it to your GitHub issue. The archive bundles runtime logs, rotated logs, performance summaries, saved app state, invalid state backups, and any matching macOS crash / hang / spin reports.

If you need to collect logs manually, Codux writes runtime logs to:

- `~/Library/Application Support/Codux/logs/runtime.log`
- `~/Library/Application Support/Codux/logs/runtime.previous.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`

Notes:

- Codux clears the previous app session logs on each launch
- `runtime.previous.log` only appears once the current session log rotates
- `performance-summary.json` covers recent performance spikes / main-thread stalls

Open the log folder directly:

```bash
open ~/Library/Application\ Support/Codux/logs
```

If the app crashes or hangs right after launch, macOS may write a system crash report to `~/Library/Logs/DiagnosticReports/` (look for `Codux-*.ips` or `dmux-*.ips`). Attach the file whose timestamp is closest to the crash.

```bash
open ~/Library/Logs/DiagnosticReports
```

When opening an issue, please include: macOS version + Codux version, repro steps, `runtime.log`, `runtime.previous.log` (if present), `performance-summary.json` (if present), and the matching crash report (if any).

---

## GitHub Star Trend

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  Wanted to be dmux, but that name was taken. So it's Codux now, which sounds like "Cool Dux" in Chinese.
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
