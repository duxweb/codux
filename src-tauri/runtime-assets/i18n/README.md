This directory contains the Tauri app's portable i18n bundle.

The bundle was seeded once from the macOS language catalog. From this point on,
maintain these JSON files directly as Codux's cross-platform language
bundle.

`locales.json` is only the manifest: it defines the supported locales and the
compiled shard list. The actual strings live under `locales/*.json`, split by
key prefix such as `common`, `git`, `settings`, and `pet`.

When adding a new language key, edit the shard matching its prefix. If a new
prefix is needed, add a new `locales/<prefix>.json`, register it in
`locales.json`, and add it to `I18N_SHARDS` in `src-tauri/src/i18n.rs`.
