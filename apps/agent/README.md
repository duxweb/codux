# Codux Host (`codux`)

The headless Codux host. Run it on any machine — a server, a spare Mac, a Linux
box — and your desktop Codux apps can connect to it to run that machine's
terminals, Git, AI sessions and memory remotely, over an end-to-end encrypted
iroh link.

It is the non-GUI counterpart of the desktop app: same protocol, same transport,
no window. One binary, `codux`, drives everything.

## Cross-platform

Builds and runs on **macOS, Linux and Windows** (x86_64 and arm64). PTYs use
`portable-pty` (ConPTY on Windows), and transport/Git/AI are all
platform-neutral. The service installer targets launchd (macOS), `systemd --user`
(Linux) and Task Scheduler (Windows).

## Install

One line (macOS / Linux) — downloads the right prebuilt binary, installs it as
`codux` on your `PATH`, no build toolchain needed:

```bash
curl -fsSL https://raw.githubusercontent.com/duxweb/codux/main/apps/agent/scripts/install.sh | sh
```

Flags: `--beta` (newest pre-release), `--version <x.y.z>` (pin a version),
`--dir <path>` (install location), `--setup` (run `config` + `install` after),
`--mirror <prefix>` (prepend a download mirror if GitHub is slow where you are).
Pass them after `sh -s --`, e.g. install the beta and set it up as a service:

```bash
curl -fsSL https://raw.githubusercontent.com/duxweb/codux/main/apps/agent/scripts/install.sh | sh -s -- --beta --setup
```

Uninstall — stops the host, removes its OS service, deletes the binary (add
`--purge` to also wipe `~/.codux-agent` config + pairings):

```bash
curl -fsSL https://raw.githubusercontent.com/duxweb/codux/main/apps/agent/scripts/install.sh | sh -s -- --uninstall
```

Or do it by hand — download the `codux-agent-<version>-<os>-<arch>` binary from
[Releases](https://github.com/duxweb/codux/releases), put it on your `PATH`, then:

```bash
codux config     # set it up (device name, relay)
codux install    # run as a startup service
codux qrcode     # show the pairing QR for your phone/desktop
```

Build from source:

```bash
cargo build -p codux-agent --release   # produces `codux-agent`
```

## Commands

| Command | What it does |
|---|---|
| `codux version` | Print the version and protocol revision. |
| `codux config` | Interactive setup wizard. Writes `codux.toml`, reusing existing values as defaults. Steps: device name → relay network → (custom relay: URL + reachability check + optional auth). |
| `codux start` | Start the host in the foreground (ASCII banner + logs). Idempotent — if one is already running it prints where, instead of starting a second. |
| `codux stop` | Stop the running host. |
| `codux status` | Whether the host is running, since when, the node id, and how many devices are paired. |
| `codux install` | Register `codux start` with the OS service manager so it starts at login and restarts on failure. |
| `codux uninstall` | Stop and remove the service. |
| `codux qrcode` | Print the pairing QR in the terminal (starts the host first if needed). Pairing is auto-confirmed — the one-time ticket is the gate. |
| `codux link` | Print the pairing ticket as text, to paste into the desktop's "Connect" box. |
| `codux update` | Check GitHub Releases for a newer build, then download, verify, replace this binary, and restart the host. |
| `codux device` | Table of paired devices (id, name, type, last seen). |
| `codux device:del <id>` | Remove a paired device. |
| `codux device:rename <id>` | Rename a paired device (prompts for the new name). |
| `codux device:clear` | Remove every paired device. |

Run `codux <command> --help` for details.

## How it works

- **Single instance.** The running host holds an advisory lock
  (`codux.lock`); `start` and the pairing commands use it to detect "already
  running" rather than starting a second copy.
- **Stable identity.** `host_id` + `host_token` (generated once by `config`)
  seed the iroh node identity, so the pairing ticket and every saved desktop's
  reconnect target survive restarts. They are never rotated automatically.
- **Pairing.** The host publishes its `codux://pair` ticket to
  `pair-ticket.json` on start; `link` and `qrcode` read it. Because reaching the
  host already requires the one-time iroh ticket, pairing is auto-confirmed.
- **Auto-recovery.** A desktop that drops and reconnects resumes the *same* host
  terminal sessions — the PTYs (and their running shells/AI) stay alive across a
  client disconnect.

## Files (`~/.codux-agent`, or `$CODUX_AGENT_DATA_DIR`)

```
codux.toml         configuration (from `codux config`)
devices.json       paired devices
pair-ticket.json   the published pairing ticket
status.json        running daemon status (pid, start time, node)
codux.lock         single-instance lock
codux.log          background daemon log
```

## Smoke tests

```bash
codux smoke pty         # spawn a PTY and read its output
codux smoke transport   # in-process iroh host↔controller round trip
codux smoke serve       # full controlled-host domain verification
```

## Boundary

Keep the agent thin. Shared host behavior belongs in
`crates/codux-runtime-core`, transport in `crates/codux-remote-transport`, PTY in
`crates/codux-terminal-pty`. The agent is just the CLI, config, service, and
state-file glue around them.
