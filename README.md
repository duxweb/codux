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
  <a href="https://github.com/duxweb/codux/issues">Feedback</a>
</p>

<p align="center">
  English | <a href="README.zh-CN.md">简体中文</a>
</p>

---

![Codux](docs/images/screenshot.png)

## Why Codux?

In the age of AI-assisted development, your IDE is no longer the center of your workflow — **the terminal is**.

Tools like Claude Code, GitHub Copilot CLI, Cursor, and Aider are turning the terminal into your primary development environment. But traditional terminals weren't designed for this:

- **Tab and split management is painful** — juggling multiple sessions across projects is slow and clunky
- **No project awareness** — no way to organize, switch, or get notifications per project
- **Git needs a separate app** — you're constantly switching between terminal, GitKraken, or Tower
- **AI usage is a black box** — you have no idea how many tokens you've burned today or which model is draining your quota
- **Electron-based alternatives are resource hogs** — they eat RAM and battery for what should be a lightweight tool

**Codux fixes all of this.** A native macOS terminal workspace purpose-built for AI CLI tools — multi-project, multi-pane, with built-in Git and real-time AI usage tracking. No Electron. No WebKit. Just pure SwiftUI + AppKit, fast and light.

## Features

### Multi-Project Workspace

Organize all your projects in one place. Each project gets its own terminal sessions, split layout, and state — everything is saved and restored automatically. Real-time activity monitoring watches your AI tools across all projects and notifies you when tasks complete, so you can context-switch without missing a beat.

### Flexible Split Panes

Split terminals horizontally, add bottom tabs, drag to resize. Work on multiple tasks within the same project without losing context.

### Built-in Git Panel

Branches, staged changes, file diffs, commit history, remote sync — all in a sidebar. No more switching to a separate Git GUI.

### AI Usage Dashboard

Track AI coding tools running in your terminal — token consumption, model usage, tool breakdowns, daily trends, and live session monitoring. Currently supports **Claude Code**, **Codex (OpenAI)**, and **Gemini CLI**, with more tools coming soon. Know exactly where your AI budget goes. Plus a fun daily tier system (Iron → Bronze → Silver → Gold → Platinum → Diamond → Master → Grandmaster) that ranks your AI usage intensity — how far can you climb today?

### Beautiful & Intuitive

Crafted with attention to every pixel. Glass vibrancy backgrounds, smooth animations, carefully balanced typography, and a clean visual hierarchy that stays out of your way. Light and dark mode are fully polished — not an afterthought. Customizable terminal themes, app icons, and keyboard shortcuts let you make it yours.

### Native & Lightweight

100% SwiftUI + AppKit. No Electron, no WebKit, no hidden browser eating your RAM. Launches instantly, idles at near-zero CPU, and respects your battery. This is what a macOS app should feel like.

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

For AI runtime regression work, see [docs/runtime-regression.md](docs/runtime-regression.md). A one-shot developer entrypoint is also available at `./scripts/dev/runtime-regression.sh`.

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

- `~/Library/Application Support/dmux/logs/dmux-debug.log`
- `~/Library/Application Support/dmux/logs/dmux-debug.previous.log`
- `~/Library/Application Support/dmux/logs/performance-summary.json`

Notes:

- Codux clears the previous app session logs on each launch, then starts a fresh runtime log for the current session
- `dmux-debug.previous.log` only appears if the current session log grows large enough to rotate
- `performance-summary.json` contains recent performance spike / main-thread stall summaries for the current session

You can also open the current log file from the app via the top bar action: `Debug Log`.

Or run this command in Terminal to open the log folder directly:

```bash
open ~/Library/Application\ Support/dmux/logs
```

### Crash Reports

If the app is frozen or unresponsive, still export diagnostics first from:

- `Help -> Export Diagnostics…`

If the app crashes or becomes unresponsive right after launch, macOS may generate a crash report here:

- `~/Library/Logs/DiagnosticReports/`

In most cases, the file you need will be named like one of these:

- `dmux-YYYY-MM-DD-*.ips`
- `dmux-bin-YYYY-MM-DD-*.ips`

If there are multiple files, please attach the one whose timestamp is closest to the time of the crash.

To open the crash report folder directly, run:

```bash
open ~/Library/Logs/DiagnosticReports
```

### Recommended Issue Attachments

Please attach or paste:

1. Your macOS version and Codux version
2. Steps to reproduce the issue
3. `dmux-debug.log`
4. `dmux-debug.previous.log` if it exists
5. `performance-summary.json` if it exists
6. The matching crash report from `~/Library/Logs/DiagnosticReports/` if the app crashed

If convenient, compress the relevant files into a single `.zip` before submitting the issue.

---

<p align="center">
  Wanted to be dmux, but that name was taken. So it's Codux now, which sounds like "Cool Dux" in Chinese.
</p>

<p align="center">
  <a href="https://codux.dux.cn">codux.dux.cn</a>
</p>
