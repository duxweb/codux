# Codux Apps

This directory contains runnable Codux applications. Shared protocol, runtime, terminal, and transport code lives in `../crates`; app directories should only own product-specific UI, packaging, platform integration, or process entry points.

## Applications

| Path | Role | Runtime |
| --- | --- | --- |
| `desktop/` | Rust + GPUI desktop app. Owns the primary UI, local workspace orchestration, AI CLI sessions, local terminal adapter, and host-side remote runtime. | Rust |
| `mobile/` | Flutter mobile controller. Connects to a Codux host, renders remote runtime state, and sends user intent. | Flutter + Rust FFI |
| `agent/` | Headless controlled-agent entry point. Uses shared protocol, transport, runtime, and PTY crates without GPUI. | Rust |

## Commands

Run from the repository root:

```bash
just desktop
just mobile android
just mobile ios
just agent -- --version
just test
```

Use app-local commands only when working inside that app's native toolchain, such as `flutter test` in `apps/mobile`.

## Release Flow

Codux uses this repository as the source-of-truth monorepo, but each shipped product is released by the repository that owns its distribution channel.

| Product | Source | Release repository | Trigger | Output |
| --- | --- | --- | --- | --- |
| Desktop | `apps/desktop` and shared `crates` | `duxweb/codux` | Push `vX.Y.Z` tag in this repository | Desktop GitHub Release assets, updater metadata, signed macOS follow-up workflow, Homebrew tap update |
| Mobile | `apps/mobile` and shared `crates` | `duxweb/codux-flutter` | Push the same `vX.Y.Z` tag in the mobile release shell repository | Android APK release and iOS TestFlight upload; workflow checks out `duxweb/codux@vX.Y.Z` |
| Service | `duxweb/codux-service` | `duxweb/codux-service` | Push `vX.Y.Z` tag in the service release repository | Linux/macOS service tarballs and GHCR Docker image |

Release checklist:

1. Update versions and changelogs in this repository.
2. Run release validation from this repository, at minimum `cargo check -p codux`, `cargo test -p codux-runtime`, and `cd apps/mobile && flutter analyze`.
3. Commit and push `main` in this repository.
4. Create or move the source tag in this repository, then push it: `git tag vX.Y.Z && git push origin vX.Y.Z`.
5. In the `duxweb/codux-flutter` release shell repository, push the same tag to trigger mobile release: `git tag vX.Y.Z && git push origin vX.Y.Z`.
6. In the `duxweb/codux-service` release repository, push the service tag when a service release is needed: `git tag vX.Y.Z && git push origin vX.Y.Z`.
7. Watch all three repositories' GitHub Actions. If a release shell workflow needs to be rerun without changing source, use `workflow_dispatch` with the same version so it checks out the monorepo tag again.

Do not add mobile or service release workflows back into this repository unless their release repositories are intentionally retired. The release shell repositories exist so their existing signing secrets, store credentials, package ownership, GHCR publishing, and release history stay in the right place.

## Ownership Rules

- UI code belongs in an app directory.
- Shared protocol names, payload shapes, transport rules, terminal state, and reusable runtime models belong in `../crates`.
- Do not duplicate Iroh transport selection, terminal sequence handling, or remote PTY restore logic in app code.
- Keep generated build output out of version control.
