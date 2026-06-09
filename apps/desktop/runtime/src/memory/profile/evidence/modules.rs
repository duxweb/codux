fn infer_project_modules(root: &Path, evidence: &ProjectProfileEvidence) -> Vec<String> {
    let mut modules = Vec::new();
    for path in [
        "src",
        "app",
        "app/Http/Controllers",
        "routes",
        "config",
        "database/migrations",
        "modules",
        "packages",
        "cmd",
        "internal",
        "pkg",
    ] {
        modules.extend(
            source_module_names(&root.join(path))
                .into_iter()
                .map(|name| {
                    if path == "src" {
                        name
                    } else {
                        format!("{path}/{name}")
                    }
                }),
        );
    }
    if has_php_framework(evidence, "laravel") {
        for path in ["app/Models", "app/Providers", "resources/views", "tests"] {
            if root.join(path).is_dir() {
                modules.push(path.to_string());
            }
        }
    }
    sorted_unique_strings(modules)
        .into_iter()
        .take(28)
        .collect()
}
