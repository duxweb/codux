fn has_php_framework(evidence: &ProjectProfileEvidence, framework: &str) -> bool {
    let deps = composer_dependencies(evidence.composer.as_ref());
    match framework {
        "laravel" => {
            deps.iter().any(|dep| dep == "laravel/framework")
                || evidence_has_path(evidence, "artisan")
        }
        "symfony" => {
            deps.iter().any(|dep| dep.starts_with("symfony/"))
                || evidence_has_path(evidence, "bin")
                    && evidence.directories.iter().any(|dir| dir == "config")
        }
        "thinkphp" => deps.iter().any(|dep| dep == "topthink/framework"),
        "dux" => {
            deps.iter()
                .any(|dep| dep.contains("dux") || dep.starts_with("duxweb/"))
                || json_string_field(evidence.composer.as_ref(), "name")
                    .is_some_and(|name| name.to_lowercase().contains("dux"))
        }
        _ => false,
    }
}

fn evidence_has_path(evidence: &ProjectProfileEvidence, name: &str) -> bool {
    evidence.directories.iter().any(|dir| dir == name) || evidence.root_markers.contains(name)
}

fn contains_token(text: &str, token: &str) -> bool {
    text.to_lowercase().contains(&token.to_lowercase())
}

pub(super) fn bullet_lines(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("- {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sorted_unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect::<Vec<_>>();
    output.sort();
    output
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}
