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

<table align="center">
<tr>
  <td align="center"><img src="docs/images/ai-stats.png" width="360" alt="AI Stats"><br/><sub>AI Stats &amp; Session Restore</sub></td>
  <td align="center"><img src="docs/images/level.png" width="360" alt="Daily Level"><br/><sub>Daily Level</sub></td>
</tr>
<tr>
  <td align="center"><img src="docs/images/git.png" width="360" alt="Built-in Git"><br/><sub>Built-in Git</sub></td>
  <td align="center"><img src="docs/images/pet.png" width="360" alt="Pet Companion"><br/><sub>Pet Companion</sub></td>
</tr>
</table>

## Demo Video

GitHub README does not render third-party iframe players. Watch the demo on [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/).

## 10 Highlights

| # | Feature | What it does |
|:--|:--|:--|
| 1 | **Live AI Activity** | Real-time status + system notifications for every running AI terminal (Claude Code, Codex, Gemini CLI, OpenCode). The tab indicator, project tile, and macOS notification all light up the moment a turn finishes — no more watching the cursor blink. |
| 2 | **AI Stats & Session Restore** | Token totals split by tool / model / project, daily and trend views, and **one-click resume** of any past session back into the original tool. Scattered AI runs turn into a usable history. |
| 3 | **Daily Level** | A daily ladder powered by real token usage. One snapshot tells you what you ran, how much, and how today compares to a normal day — easy to glance at, hard to fudge. |
| 4 | **Pet Companion** | An optional pet in the title bar that grows with your AI coding habits. Different coding styles unlock different growth values and roadmaps, and the pet chimes in once in a while so long sessions don't feel lonely. Fully optional, one-click mute. |
| 5 | **Built-in Git** | A first-class Git panel — not an embedded webview. Branch checkout / create / rename / delete, staging with line-level diffs, full commit history, and push / pull / sync with sane defaults and clear conflict resolution. |
| 6 | **Project File Browser** | Per-project native file manager. Edit code inline, preview images and other assets, and drag any file straight into the terminal so your AI tool gets the right path on the first try. |
| 7 | **Multi-Project Workspaces** | Every project is its own room — up to **6 split terminals** for parallel work plus **unlimited tabs** when 6 is not enough. Each project keeps its own layout, sessions, AI tool selection, and state across restarts. |
| 8 | **Three-Layer AI Memory** | Local `memory.sqlite3` extracts long-term memory from completed sessions, layered as **user / project / tool**. App-private `CLAUDE.md`, `AGENTS.md`, `GEMINI.md` are generated so Codex / Claude / Gemini / OpenCode no longer forget what you did last session — and nothing is written into your repo. |
| 9 | **Mobile Handoff** | Step away from the Mac and keep going on your phone. Codux Mobile pairs with the Mac host to drive AI CLI sessions remotely with end-to-end encrypted relay traffic. See the [Mobile Handoff](#mobile-handoff) section below. |
| 10 | **Ghostty Engine & Themes** | Embeds the [`ghostty`](https://ghostty.org) terminal engine for GPU-accelerated rendering, plus a curated set of light / dark themes that follow macOS appearance. |

## Mobile Handoff

Codux Mobile + Codux Service are a separate stack so the relay can be self-hosted while the Mac stays the real terminal host.

| Component | Purpose | Download |
|:--|:--|:--|
| Codux for macOS | Main desktop app: projects, terminals, Git, stats, memory, remote host. | [macOS Releases](https://github.com/duxweb/codux/releases) |
| Codux Mobile | Android client: pair with the Mac, run AI CLI sessions remotely, browse files, upload images. | [Mobile Releases](https://github.com/duxweb/codux-flutter/releases) |
| Codux Service | Lightweight Go relay for device pairing and encrypted WebSocket forwarding. | [Service Releases](https://github.com/duxweb/codux-service/releases) |

For a quick trial, enter one of the official trial relays in **Settings > Remote**:

| Node | URL |
|:--|:--|
| China relay direct | `https://codux-service.dux.plus` |
| Global transit acceleration | `https://codux-node.dux.plus` |

Terminal input, output, file payloads, project lists, and AI stats are end-to-end encrypted between Codux for macOS and Codux Mobile. The relay sees only routing metadata (host ID, device ID, pairing state, online state) — never decrypted terminal content. For long-term use, self-hosting `codux-service` is recommended.

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
