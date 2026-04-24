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
- **AI tools forget project context** — useful decisions, conventions, and bug lessons get buried in terminal history
- **Electron-based alternatives are resource hogs** — they eat RAM and battery for what should be a lightweight tool

**Codux fixes all of this.** A native macOS terminal workspace purpose-built for AI CLI tools — multi-project, multi-pane, with built-in Git and real-time AI usage tracking. No Electron. No WebKit. Just pure SwiftUI + AppKit, fast and light.

## Features

| Screenshot | Feature | Description |
|:--|:--|:--|
| ![Codux Split Workspace](docs/images/screenshot.png) | Multi-Project Workspace | Organize all your projects in one place. Each project keeps its own terminal sessions, split layout, and state, with activity monitoring that helps you switch context without missing task completion. |
| ![Codux Git Panel](docs/images/git.png) | Built-in Git Panel | Manage branches, staged changes, file diffs, commit history, and remote sync directly in the sidebar instead of jumping to a separate Git app. |
| ![Codux AI Stats](docs/images/ai-stats.png) | AI Usage Dashboard | Track token usage, model usage, tool breakdowns, daily trends, and live sessions for AI coding tools running inside your terminal. |
| ![Codux AI Stats](docs/images/ai-stats.png) | AI Memory | Extract durable user and project memory from completed AI sessions, then inject concise context and global prompts into supported tools when new terminal sessions start. |
| ![Codux Daily Level](docs/images/level.png) | Daily Level Ladder | See your current daily AI usage rank at a glance with the live ladder: `Idle`, `Light`, `Active`, `Focus`, `Intense`, `Grind`, `Limit`, `Godlike`. |
| ![Codux Pet System](docs/images/pet.png) | Coding Pet Companion | Claim an egg, grow a companion, unlock evolutions, fill the dex, and get contextual bubble reactions while you work. |

### Beautiful & Intuitive

Crafted with attention to every pixel. Glass vibrancy backgrounds, smooth animations, carefully balanced typography, and a clean visual hierarchy that stays out of your way. Light and dark mode are fully polished — not an afterthought. Customizable terminal themes, app icons, and keyboard shortcuts let you make it yours.

### Native & Lightweight

100% SwiftUI + AppKit. No Electron, no WebKit, no hidden browser eating your RAM. Launches instantly, idles at near-zero CPU, and respects your battery. This is what a macOS app should feel like.

### AI Memory

Codux can turn completed AI coding sessions into durable memory. It stores user preferences, project conventions, decisions, facts, and bug lessons locally in SQLite, then prepares concise launch context for supported tools. Memory is injected from Codux-managed runtime files, not written into your repository, and it is treated as guidance while the current repo remains the source of truth.

Highlights:

- Local `memory.sqlite3` storage for user and project memory
- Automatic extraction from completed Claude, Codex, Gemini, and OpenCode sessions
- Automatic extraction provider selection, with per-provider test buttons in **Settings > AI**
- Global prompt support for all supported tools
- App-private `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` launch context generation
- Title-bar memory status indicator for queued, running, and failed extraction states

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

## Pet System

### What It Is

The pet system is a lightweight companion layer built into Codux. It turns your everyday AI coding activity into a long-term progression loop: hatch an egg, grow a companion, unlock evolutions, fill the dex, and eventually inherit your fully grown pet into history.

### Current Pet Lineup

| Species | Base Stages | Route A | Route B |
|:--|:--|:--|:--|
| `VoidCat` | `Huahua` → `Shadow Cat` → `Voidcat` | `Tomecat` → `Inkspirit` | `Shadecat` → `Nightspirit` |
| `RustHound` | `Furball` → `Flop-Eared Pup` → `Rusthound` | `Blazehound` → `Sunflare` | `Ironwolf` → `Bloodmoon` |
| `Goose` | `Chirpy` → `Dozy` → `Goosey` | `Dawnwing` → `Wildfire` | `Windwing` → `Tempest` |

### How To Claim A Pet

- Open Codux and look at the title bar pet button.
- If you have not claimed a pet yet, click the pet button to open the egg claim dialog.
- Choose one egg, optionally enter a custom name, then confirm the claim.
- After claiming, the pet starts growing with your AI coding activity.

### Where To Find It

- `Title bar`: open the current pet popover, view hatching / level / traits, and access the dex
- `Pet Dex`: view unlocked stages, current pet details, and inheritance history
- `Settings > Pet`: configure pet enable state, static sprite mode, hydration reminders, sedentary reminders, and late-night reminders

### Menus And UI Entry Points

- The pet is not a separate app window by default; the main entry is the title bar pet button.
- Clicking the title bar pet button opens:
  - egg selection if no pet has been claimed
  - the pet popover if a pet is active
- The dex is opened from the pet popover.

### Hidden Pet Unlock

- One egg option is a random egg.
- The random egg has a chance to hatch a hidden species.
- The base chance is `15%`.
- If you have actively used `2` or more AI tools in the last `7` days, the chance increases directly to `50%`.

### Personality Dimensions

- `Wisdom`: longer, deeper requests and sustained high-context work
- `Chaos`: fast, frequent, short-cycle interaction
- `Night`: late-night usage patterns
- `Stamina`: long-running sessions and sustained active time
- `Empathy`: iterative back-and-forth debugging and repair-style work

### Reminders

In **Settings > Pet**, you can configure:

- hydration reminder interval
- sedentary reminder interval
- late-night reminder interval
- pet enable / disable
- static pet sprite mode

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
