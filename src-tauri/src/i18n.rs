use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

const I18N_MANIFEST_JSON: &str = include_str!("../runtime-assets/i18n/locales.json");
const I18N_SHARDS: &[(&str, &str)] = &[
    (
        "locales/about.json",
        include_str!("../runtime-assets/i18n/locales/about.json"),
    ),
    (
        "locales/agent.json",
        include_str!("../runtime-assets/i18n/locales/agent.json"),
    ),
    (
        "locales/ai.json",
        include_str!("../runtime-assets/i18n/locales/ai.json"),
    ),
    (
        "locales/app.json",
        include_str!("../runtime-assets/i18n/locales/app.json"),
    ),
    (
        "locales/common.json",
        include_str!("../runtime-assets/i18n/locales/common.json"),
    ),
    (
        "locales/diagnostics.json",
        include_str!("../runtime-assets/i18n/locales/diagnostics.json"),
    ),
    (
        "locales/files.json",
        include_str!("../runtime-assets/i18n/locales/files.json"),
    ),
    (
        "locales/git.json",
        include_str!("../runtime-assets/i18n/locales/git.json"),
    ),
    (
        "locales/memory.json",
        include_str!("../runtime-assets/i18n/locales/memory.json"),
    ),
    (
        "locales/menu.json",
        include_str!("../runtime-assets/i18n/locales/menu.json"),
    ),
    (
        "locales/open.json",
        include_str!("../runtime-assets/i18n/locales/open.json"),
    ),
    (
        "locales/performance.json",
        include_str!("../runtime-assets/i18n/locales/performance.json"),
    ),
    (
        "locales/pet.json",
        include_str!("../runtime-assets/i18n/locales/pet.json"),
    ),
    (
        "locales/project.json",
        include_str!("../runtime-assets/i18n/locales/project.json"),
    ),
    (
        "locales/rank.json",
        include_str!("../runtime-assets/i18n/locales/rank.json"),
    ),
    (
        "locales/remote.json",
        include_str!("../runtime-assets/i18n/locales/remote.json"),
    ),
    (
        "locales/settings.json",
        include_str!("../runtime-assets/i18n/locales/settings.json"),
    ),
    (
        "locales/sidebar.json",
        include_str!("../runtime-assets/i18n/locales/sidebar.json"),
    ),
    (
        "locales/sleep_prevention.json",
        include_str!("../runtime-assets/i18n/locales/sleep_prevention.json"),
    ),
    (
        "locales/ssh.json",
        include_str!("../runtime-assets/i18n/locales/ssh.json"),
    ),
    (
        "locales/startup.json",
        include_str!("../runtime-assets/i18n/locales/startup.json"),
    ),
    (
        "locales/task_memo.json",
        include_str!("../runtime-assets/i18n/locales/task_memo.json"),
    ),
    (
        "locales/terminal.json",
        include_str!("../runtime-assets/i18n/locales/terminal.json"),
    ),
    (
        "locales/titlebar.json",
        include_str!("../runtime-assets/i18n/locales/titlebar.json"),
    ),
    (
        "locales/update.json",
        include_str!("../runtime-assets/i18n/locales/update.json"),
    ),
    (
        "locales/welcome.json",
        include_str!("../runtime-assets/i18n/locales/welcome.json"),
    ),
    (
        "locales/workspace.json",
        include_str!("../runtime-assets/i18n/locales/workspace.json"),
    ),
    (
        "locales/worktree.json",
        include_str!("../runtime-assets/i18n/locales/worktree.json"),
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct I18nBundle {
    pub source_language: String,
    pub locales: Vec<String>,
    pub strings: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct I18nManifest {
    source_language: String,
    locales: Vec<String>,
    shards: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct I18nShard {
    strings: HashMap<String, HashMap<String, String>>,
}

static BUNDLE: OnceLock<I18nBundle> = OnceLock::new();

pub fn i18n_bundle() -> I18nBundle {
    BUNDLE.get_or_init(parse_i18n_bundle).clone()
}

pub fn translate(locale: &str, key: &str, fallback: &str) -> String {
    let bundle = BUNDLE.get_or_init(parse_i18n_bundle);
    bundle
        .strings
        .get(locale)
        .and_then(|strings| strings.get(key))
        .or_else(|| {
            bundle
                .strings
                .get("zh-Hans")
                .and_then(|strings| strings.get(key))
        })
        .or_else(|| {
            bundle
                .strings
                .get("en")
                .and_then(|strings| strings.get(key))
        })
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

fn parse_i18n_bundle() -> I18nBundle {
    parse_i18n_bundle_result().unwrap_or_else(|_| I18nBundle {
        source_language: "en".to_string(),
        locales: vec!["en".to_string()],
        strings: HashMap::new(),
    })
}

fn parse_i18n_bundle_result() -> Result<I18nBundle, serde_json::Error> {
    let manifest: I18nManifest = serde_json::from_str(I18N_MANIFEST_JSON)?;
    let mut strings: HashMap<String, HashMap<String, String>> = manifest
        .locales
        .iter()
        .map(|locale| (locale.clone(), HashMap::new()))
        .collect();
    let shard_lookup: HashMap<&str, &str> = I18N_SHARDS.iter().copied().collect();

    for shard_path in &manifest.shards {
        let Some(shard_json) = shard_lookup.get(shard_path.as_str()) else {
            continue;
        };
        let shard: I18nShard = serde_json::from_str(shard_json)?;
        for (locale, shard_strings) in shard.strings {
            strings.entry(locale).or_default().extend(shard_strings);
        }
    }

    Ok(I18nBundle {
        source_language: manifest.source_language,
        locales: manifest.locales,
        strings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_portable_i18n_bundle() {
        let bundle = parse_i18n_bundle();
        assert!(bundle.locales.iter().any(|locale| locale == "en"));
        assert!(bundle.locales.iter().any(|locale| locale == "zh-Hans"));
        assert_eq!(
            bundle
                .strings
                .get("zh-Hans")
                .and_then(|strings| strings.get("common.copy"))
                .map(String::as_str),
            Some("复制")
        );
        assert_eq!(
            bundle
                .strings
                .get("zh-Hans")
                .and_then(|strings| strings.get("common.cut"))
                .map(String::as_str),
            Some("剪切")
        );
        assert_eq!(
            bundle
                .strings
                .get("zh-Hans")
                .and_then(|strings| strings.get("common.redo"))
                .map(String::as_str),
            Some("重做")
        );
        assert_eq!(
            bundle
                .strings
                .get("en")
                .and_then(|strings| strings.get("titlebar.git"))
                .map(String::as_str),
            Some("Git")
        );
    }

    #[test]
    fn manifest_references_all_compiled_i18n_shards() {
        let manifest: I18nManifest = serde_json::from_str(I18N_MANIFEST_JSON).unwrap();
        let compiled: Vec<&str> = I18N_SHARDS.iter().map(|(path, _)| *path).collect();

        assert_eq!(
            manifest.shards.len(),
            compiled.len(),
            "manifest and compiled shard list must stay in sync"
        );
        for path in compiled {
            assert!(
                manifest.shards.iter().any(|item| item == path),
                "missing i18n shard in manifest: {path}"
            );
        }
    }
}
