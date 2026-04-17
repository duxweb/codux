# Release Updates

This project publishes macOS updates through Sparkle backed by GitHub Releases.

## Required Secrets

- `SPARKLE_PUBLIC_ED_KEY`
  The public Ed25519 key embedded into the app as `SUPublicEDKey`.
- `SPARKLE_PRIVATE_ED_KEY`
  The private Ed25519 key used by CI to sign `appcast.xml`.

## Release Flow

1. Update the entry for the target version in `CHANGELOG.md`.
2. Push a Git tag in the form `vX.Y.Z`.
3. GitHub Actions runs `.github/workflows/release-build.yml`.
4. The workflow builds and uploads:
   - `Codux-<version>-macos-universal.dmg`
   - `Codux-<version>-macos-universal.zip`
   - `Codux-debug-<version>-debug-macos-universal.dmg`
   - `SHA256SUMS.txt`
   - `appcast.xml`

## Release Notes Source

- `scripts/release/extract-release-notes.sh` extracts the matching version section from `CHANGELOG.md`.
- The workflow passes the extracted notes to Sparkle `generate_appcast`.
- `generate_appcast --embed-release-notes` embeds the notes directly into `appcast.xml`.

This means the Sparkle updater dialog reads release notes from the current version section in `CHANGELOG.md`, not from the GitHub release HTML page.

## Feed URL

The app uses:

- `https://github.com/duxweb/codux/releases/latest/download/appcast.xml`

For existing releases, adding or replacing `appcast.xml` may take a short time to propagate through GitHub's CDN cache.
