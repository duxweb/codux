fn extract_readme_overview(readme: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in readme.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('<')
            || trimmed.starts_with('!')
            || trimmed.starts_with('[')
            || trimmed.starts_with('#')
            || trimmed.starts_with('|')
            || trimmed.starts_with("---")
        {
            if !lines.is_empty() {
                break;
            }
            continue;
        }
        lines.push(trimmed.to_string());
        if lines.join(" ").chars().count() > 420 {
            break;
        }
    }
    normalized_string(Some(&lines.join(" ")))
}

fn infer_project_overview(project_name: &str, evidence: &ProjectProfileEvidence) -> String {
    if let Some(overview) = evidence.readme.as_deref().and_then(extract_readme_overview) {
        return overview;
    }
    if let Some(description) = json_string_field(evidence.package.as_ref(), "description") {
        return description;
    }
    if let Some(description) = json_string_field(evidence.composer.as_ref(), "description") {
        return description;
    }
    if has_php_framework(evidence, "laravel") {
        return format!(
            "{project_name} is a Laravel PHP application with Composer-managed dependencies."
        );
    }
    if has_php_framework(evidence, "symfony") {
        return format!(
            "{project_name} is a Symfony PHP application with Composer-managed dependencies."
        );
    }
    if has_php_framework(evidence, "thinkphp") {
        return format!(
            "{project_name} is a ThinkPHP application with Composer-managed dependencies."
        );
    }
    if has_php_framework(evidence, "dux") {
        return format!(
            "{project_name} is a Dux PHP application with Composer-managed dependencies."
        );
    }
    if evidence.composer.is_some() {
        return format!("{project_name} is a PHP project managed with Composer.");
    }
    if let Some(package_name) = json_string_field(evidence.package.as_ref(), "name") {
        return format!(
            "{package_name} is a JavaScript/TypeScript project managed with package scripts."
        );
    }
    if let Some(module) = go_module_name(evidence.go_mod.as_deref()) {
        return format!("{module} is a Go module.");
    }
    if evidence.pyproject.is_some() {
        return format!("{project_name} is a Python project configured with pyproject.toml.");
    }
    format!("{project_name} project workspace.")
}
