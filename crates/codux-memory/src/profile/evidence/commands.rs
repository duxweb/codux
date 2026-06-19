fn infer_project_commands(evidence: &ProjectProfileEvidence) -> Vec<String> {
    let mut commands = Vec::new();
    if let Some(scripts) = evidence
        .package
        .as_ref()
        .and_then(|value| value.get("scripts"))
        .and_then(Value::as_object)
    {
        for key in ["dev", "build", "test", "lint", "tauri"] {
            if scripts.contains_key(key) {
                commands.push(format!("{} {key}", evidence.package_manager));
            }
        }
        if scripts.contains_key("build") {
            commands.push(package_exec_command(
                &evidence.package_manager,
                "tsc --noEmit",
            ));
        }
    }
    if evidence.cargo.is_some() {
        commands.push("cargo check --manifest-path src-tauri/Cargo.toml".to_string());
    }
    if evidence.root_cargo.is_some() {
        commands.push("cargo check".to_string());
    }
    if evidence.composer.is_some() {
        commands.push("composer install".to_string());
        if has_composer_script(evidence.composer.as_ref(), "test") {
            commands.push("composer test".to_string());
        }
        if has_php_framework(evidence, "laravel") {
            commands.push("php artisan test".to_string());
        } else if has_php_framework(evidence, "symfony") {
            commands.push("php bin/console".to_string());
        }
    }
    if evidence.pyproject.is_some() {
        commands.push("python -m pytest".to_string());
    }
    if evidence.go_mod.is_some() {
        commands.push("go test ./...".to_string());
    }
    if evidence.pom.is_some() {
        commands.push("mvn test".to_string());
    }
    if evidence.gradle.is_some() {
        commands.push("./gradlew test".to_string());
    }
    if evidence.gemfile.is_some() {
        commands.push("bundle exec rspec".to_string());
    }
    sorted_unique_strings(commands)
}

fn has_composer_script(composer: Option<&Value>, script: &str) -> bool {
    composer
        .and_then(|value| value.get("scripts"))
        .and_then(Value::as_object)
        .is_some_and(|scripts| scripts.contains_key(script))
}

fn package_exec_command(manager: &str, command: &str) -> String {
    match manager {
        "pnpm" => format!("pnpm exec {command}"),
        "bun" => format!("bunx {command}"),
        "yarn" => format!("yarn {command}"),
        _ => format!("npx {command}"),
    }
}
