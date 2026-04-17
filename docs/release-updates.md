# Release Updates

This project publishes macOS updates through Sparkle backed by GitHub Releases.

## Required Secrets

- `SPARKLE_PUBLIC_ED_KEY`
  The public Ed25519 key embedded into the app as `SUPublicEDKey`.
- `SPARKLE_PRIVATE_ED_KEY`
  The private Ed25519 key used by CI to sign `appcast.xml`.
- `HOMEBREW_TAP_TOKEN`
  A GitHub token with read/write access to `duxweb/homebrew-tap`, used by the release workflow to clone the tap repository and push the updated cask after a release is published.

## Release Flow

1. Keep ongoing development notes under `## [Unreleased]` in `CHANGELOG.md`.
2. If you want bilingual notes during development, keep the matching `## [Unreleased]` section in `CHANGELOG.zh-CN.md` too.
3. When preparing a release, move the finalized `Unreleased` notes into a new `## [X.Y.Z] - YYYY-MM-DD` section in both changelog files.
4. Push a Git tag in the form `vX.Y.Z`.
5. GitHub Actions runs `.github/workflows/release-build.yml`.
6. The workflow builds and uploads:
   - `Codux-<version>-macos-universal.dmg`
   - `Codux-<version>-macos-universal.zip`
   - `Codux-debug-<version>-debug-macos-universal.dmg`
   - `SHA256SUMS.txt`
   - `appcast.xml`
7. If `HOMEBREW_TAP_TOKEN` is valid, the workflow also updates `duxweb/homebrew-tap`.

The Homebrew tap update is best-effort. If the token is missing or invalid, the main app release still succeeds and the workflow summary reports the tap failure separately.

The workflow uses direct authenticated `git clone` / `git push` for the tap repository instead of `gh repo clone`, so failures now point more directly to token validity or repository permissions.

## Release Notes Source

- `scripts/release/extract-release-notes.sh` extracts the matching version section from `CHANGELOG.md`.
- `scripts/release/build-release-notes.sh` combines `CHANGELOG.md` and `CHANGELOG.zh-CN.md` into one release-notes markdown file.
- The workflow passes the combined notes to Sparkle `generate_appcast`.
- `generate_appcast --embed-release-notes` embeds the notes directly into `appcast.xml`.

If the same version exists in both changelog files, the updater and GitHub Release body will display Chinese first and English below it. If the Chinese entry is missing, the workflow falls back to English-only notes.

This means the Sparkle updater dialog reads release notes from the current version section in the changelog files, not from the GitHub release HTML page.

## Feed URL

The app uses:

- `https://github.com/duxweb/codux/releases/latest/download/appcast.xml`

For existing releases, adding or replacing `appcast.xml` may take a short time to propagate through GitHub's CDN cache.
