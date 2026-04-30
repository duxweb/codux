<p align="center">
  <img src="docs/images/icon.png" width="128" height="128" alt="Codux">
</p>

<h1 align="center">Codux</h1>

<p align="center">
  A native macOS terminal workspace for AI coding tools.
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

<div align="center">
  <iframe src="https://player.bilibili.com/player.html?isOutside=true&aid=116493605935938&bvid=BV1mK9vBCEYD&cid=37979163134&p=1" width="100%" height="420" scrolling="no" border="0" frameborder="no" framespacing="0" allowfullscreen="true"></iframe>
</div>

If the embedded player is not available, watch it on [Bilibili](https://www.bilibili.com/video/BV1mK9vBCEYD/).

## Why Codux?

Codux is for developers who spend most of their AI coding time in terminal tools such as Claude Code, Codex, Gemini CLI, and OpenCode.

| Problem | What Codux gives you |
|:--|:--|
| Terminal windows multiply across projects | A project-aware workspace where each project keeps its own sessions, split layout, and state. |
| AI work finishes while you are in another project | Activity indicators and notifications show which project is running, waiting, or completed. |
| Git requires another app | A built-in Git panel for branches, diffs, staging, commits, history, and sync. |
| AI token usage is hard to understand | A live usage dashboard shows tool, model, request, token, and daily trend breakdowns. |
| Useful AI decisions get buried in history | Local AI memory extracts durable preferences, project conventions, and lessons from completed sessions. |
| You want to check or control work from your phone | Codux Mobile connects to the Mac host through Codux Service and runs remote terminal sessions securely. |
| Electron terminals feel heavy | Codux is native SwiftUI + AppKit. No Electron, no WebKit terminal surface. |

## Core Features

| Area | What You Get | Where It Lives |
|:--|:--|:--|
| Multi-project terminal workspace | Per-project terminals, tabs, splits, restored state, and project activity status. | Main window |
| Built-in Git | Branches, staged changes, diffs, commit history, and remote sync. | Sidebar Git panel |
| AI usage dashboard | Token totals, model breakdowns, tool rankings, daily trends, and live session state. | AI panel |
| AI memory | Local `memory.sqlite3` storage, automatic extraction, provider tests, global prompts, and app-private launch context for supported tools. | Settings > AI and title-bar memory indicator |
| Remote workspace | Pair mobile devices, create mobile-only terminal sessions on the Mac host, browse files, upload images, and operate terminals from Android. | Settings > Remote and Codux Mobile |
| Daily level | A lightweight daily usage ladder from `Idle` to `Godlike`. | Title bar / AI stats |
| Pet companion | Optional companion that grows with AI coding activity and shows contextual reminders. | Title bar pet button and Settings > Pet |

## Screenshots

| Workspace | Git | AI Usage |
|:--|:--|:--|
| ![Codux Split Workspace](docs/images/screenshot.png) | ![Codux Git Panel](docs/images/git.png) | ![Codux AI Stats](docs/images/ai-stats.png) |

| Daily Level | Pet |
|:--|:--|
| ![Codux Daily Level](docs/images/level.png) | ![Codux Pet System](docs/images/pet.png) |

## Remote Access

Codux remote access is split into three parts so you can self-host the relay and keep the Mac as the real terminal host.

| Component | Purpose | Download |
|:--|:--|:--|
| Codux for macOS | Main desktop app. It owns projects, terminals, Git, AI stats, memory, and remote host sessions. | [macOS Releases](https://github.com/duxweb/codux/releases) |
| Codux Mobile | Android client for pairing with the Mac, opening remote terminal sessions, browsing project files, and uploading images. | [Mobile Releases](https://github.com/duxweb/codux-flutter/releases) |
| Codux Service | Lightweight Go relay for device pairing and encrypted WebSocket message forwarding. | [Service Releases](https://github.com/duxweb/codux-service/releases) |

For a quick trial, enter one of the official trial relays in **Settings > Remote**:

| Node | URL |
|:--|:--|
| China relay direct | `https://codux-service.dux.plus` |
| Global transit acceleration | `https://codux-node.dux.plus` |

For production or long-term use, self-hosting `codux-service` is recommended.

### Remote Setup Flow

| Step | Action |
|:--|:--|
| 1 | Use one of the official trial relays above, or deploy `codux-service` on your own server, VPS, or LAN machine. |
| 2 | Put HTTPS/WSS in front of the service for production use. Edge/CDN products can proxy WebSocket traffic, but the Go relay should run as a normal long-lived process. |
| 3 | Open Codux for macOS, go to **Settings > Remote**, enter the relay server URL, and enable remote access. |
| 4 | Click the pairing button to show a one-time QR code. |
| 5 | Install Codux Mobile, scan the QR code, compare the matching code, and confirm the device on macOS. |
| 6 | Use Codux Mobile to open remote terminal sessions that run on the Mac host without inserting those sessions into the visible Mac split layout. |

Remote terminal input, output, file payloads, project lists, and AI stats are wrapped as end-to-end encrypted payloads between Codux for macOS and Codux Mobile. The relay sees routing metadata such as host ID, device ID, pairing state, and online state, but not decrypted terminal content.

## AI Memory

Codux can turn completed AI coding sessions into durable memory. It stores user preferences, project conventions, decisions, facts, and bug lessons locally in SQLite, then prepares concise launch context for supported tools. Memory is injected from Codux-managed runtime files, not written into your repository, and the current repo remains the source of truth.

| Capability | Details |
|:--|:--|
| Storage | Local `memory.sqlite3` for user and project memory. |
| Extraction | Completed Claude, Codex, Gemini, and OpenCode sessions can be summarized automatically. |
| Provider control | Automatic provider selection plus per-provider test buttons in **Settings > AI**. |
| Launch context | App-private `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` are generated for supported tool launches. |
| Status | The title bar shows queued, running, and failed memory extraction states. |

## Pet Companion

The pet system is optional and intentionally lightweight. It turns AI coding activity into a small companion loop without becoming a separate game screen.

| Item | Description |
|:--|:--|
| Entry | Click the title-bar pet button. If no pet exists, Codux opens the egg claim dialog. |
| Growth | AI coding activity contributes to hatching, level growth, traits, evolutions, dex unlocks, and inheritance history. |
| Reminders | Hydration, sedentary, and late-night reminders can be configured in **Settings > Pet**. |
| Control | You can disable the pet or switch to static sprite mode in **Settings > Pet**. |

## Native macOS Experience

Codux is built with SwiftUI + AppKit. It launches quickly, idles quietly, supports light/dark mode, and keeps terminal, Git, AI stats, memory, and remote state in one native workspace.

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

When reporting a bug, please include the following diagnostics whenever possible:

### App Logs

The easiest way is:

- Open `Help -> Export Diagnostics…`
- Save the generated `.zip`
- Attach that archive to your GitHub issue

The diagnostics archive includes the most important files for troubleshooting, including:

- app runtime logs
- previous rotated logs
- performance event summary from the current app session
- saved app state files
- invalid state backups when available
- related crash / hang / spin reports from macOS when available

If you need to collect logs manually, Codux writes runtime logs to:

- `~/Library/Application Support/Codux/logs/runtime.log`
- `~/Library/Application Support/Codux/logs/runtime.previous.log`
- `~/Library/Application Support/Codux/logs/performance-summary.json`

Notes:

- Codux clears the previous app session logs on each launch, then starts a fresh runtime log for the current session
- `runtime.previous.log` only appears if the current session log grows large enough to rotate
- `performance-summary.json` contains recent performance spike / main-thread stall summaries for the current session

You can also open the current log file from the app via the top bar action: `Debug Log`.

Or run this command in Terminal to open the log folder directly:

```bash
open ~/Library/Application\ Support/Codux/logs
```

### Crash Reports

If the app is frozen or unresponsive, still export diagnostics first from:

- `Help -> Export Diagnostics…`

If the app crashes or becomes unresponsive right after launch, macOS may generate a crash report here:

- `~/Library/Logs/DiagnosticReports/`

In most cases, the file you need will be named like one of these:

- `Codux-YYYY-MM-DD-*.ips`
- `dmux-YYYY-MM-DD-*.ips`

If there are multiple files, please attach the one whose timestamp is closest to the time of the crash.

To open the crash report folder directly, run:

```bash
open ~/Library/Logs/DiagnosticReports
```

### Recommended Issue Attachments

Please attach or paste:

1. Your macOS version and Codux version
2. Steps to reproduce the issue
3. `runtime.log`
4. `runtime.previous.log` if it exists
5. `performance-summary.json` if it exists
6. The matching crash report from `~/Library/Logs/DiagnosticReports/` if the app crashed

If convenient, compress the relevant files into a single `.zip` before submitting the issue.

---

## GitHub Star Trend

[![Star History Chart](https://api.star-history.com/svg?repos=duxweb/codux&type=Date)](https://star-history.com/#duxweb/codux&Date)

<p align="center">
  Wanted to be dmux, but that name was taken. So it's Codux now, which sounds like "Cool Dux" in Chinese.
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
