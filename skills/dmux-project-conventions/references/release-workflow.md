# Release Workflow

dmux publishes macOS updates through Sparkle backed by GitHub Releases.

## Required secrets

- `SPARKLE_PUBLIC_ED_KEY`
  Embedded as `SUPublicEDKey`
- `SPARKLE_PRIVATE_ED_KEY`
  Used by CI to sign `appcast.xml`
- `HOMEBREW_TAP_TOKEN`
  Updates `duxweb/homebrew-tap` after a release

## Release steps

1. Keep in-progress notes under `## [Unreleased]` in both changelog files.
2. When cutting a release, move finalized notes into `## [X.Y.Z] - YYYY-MM-DD` in:
   - `CHANGELOG.md`
   - `CHANGELOG.zh-CN.md`
3. Push tag `vX.Y.Z`.
4. GitHub Actions runs `.github/workflows/release-build.yml`.
5. Release artifacts are built and uploaded:
   - `Codux-<version>-macos-universal.dmg`
   - `Codux-<version>-macos-universal.zip`
   - `SHA256SUMS.txt`
   - `appcast.xml`
   Debug DMGs are not part of the formal release asset workflow; use the manual test-build workflow when a debug package is needed.
6. If `HOMEBREW_TAP_TOKEN` is valid, the workflow also updates the Homebrew cask repo.

## Release notes source

- `scripts/release/extract-release-notes.sh`
  Extracts the matching version section from `CHANGELOG.md`
- `scripts/release/build-release-notes.sh`
  Combines English + Chinese changelog sections
- Sparkle `generate_appcast --embed-release-notes`
  Embeds notes directly into `appcast.xml`

If both changelog files contain the same version, Chinese appears first and English below it. If the Chinese entry is missing, the release falls back to English only.

## Feed URL

- `https://github.com/duxweb/codux/releases/latest/download/appcast.xml`
