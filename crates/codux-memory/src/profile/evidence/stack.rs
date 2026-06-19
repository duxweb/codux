fn infer_project_tech_stack(evidence: &ProjectProfileEvidence) -> Vec<String> {
    let mut stack = Vec::new();
    let deps = package_dependencies(evidence.package.as_ref());
    let composer_deps = composer_dependencies(evidence.composer.as_ref());
    if deps.iter().any(|dep| dep == "react" || dep == "react-dom") {
        stack.push("Frontend: React".to_string());
    }
    if deps
        .iter()
        .any(|dep| dep == "vue" || dep.starts_with("@vitejs/plugin-vue"))
    {
        stack.push("Frontend: Vue".to_string());
    }
    if deps.iter().any(|dep| dep == "next") {
        stack.push("Framework: Next.js".to_string());
    }
    if deps.iter().any(|dep| dep == "nuxt") {
        stack.push("Framework: Nuxt".to_string());
    }
    if deps.iter().any(|dep| dep == "typescript") {
        stack.push("Language: TypeScript".to_string());
    }
    if evidence.package.is_some() {
        stack.push(format!("Package manager: {}", evidence.package_manager));
    }
    if deps
        .iter()
        .any(|dep| dep == "vite" || dep == "@vitejs/plugin-react")
    {
        stack.push("Build: Vite".to_string());
    }
    if deps
        .iter()
        .any(|dep| dep == "tailwindcss" || dep == "@tailwindcss/vite")
    {
        stack.push("Styling: Tailwind CSS".to_string());
    }
    if deps.iter().any(|dep| dep == "zustand") {
        stack.push("State: Zustand".to_string());
    }
    if deps
        .iter()
        .any(|dep| dep.starts_with("@codemirror/") || dep == "codemirror")
    {
        stack.push("Editor: CodeMirror".to_string());
    }
    if deps.iter().any(|dep| dep.starts_with("@xterm/")) {
        stack.push("Terminal: xterm.js".to_string());
    }
    if evidence.tauri.is_some() || deps.iter().any(|dep| dep.starts_with("@tauri-apps/")) {
        stack.push("Desktop: Tauri".to_string());
    }
    if evidence.cargo.is_some() || evidence.root_cargo.is_some() {
        stack.push("Native/runtime: Rust".to_string());
    }
    if cargo_dependency_present(evidence, "genai") {
        stack.push("AI provider SDK: genai".to_string());
    }
    if cargo_dependency_present(evidence, "rusqlite") {
        stack.push("Storage: SQLite".to_string());
    }
    if evidence.composer.is_some() {
        stack.push("Language: PHP".to_string());
        stack.push("Package manager: Composer".to_string());
    }
    if composer_deps.iter().any(|dep| dep == "laravel/framework")
        || evidence_has_path(evidence, "artisan")
    {
        stack.push("Framework: Laravel".to_string());
    }
    if composer_deps.iter().any(|dep| dep.starts_with("symfony/"))
        || evidence_has_path(evidence, "bin")
    {
        stack.push("Framework: Symfony".to_string());
    }
    if composer_deps.iter().any(|dep| dep == "topthink/framework") {
        stack.push("Framework: ThinkPHP".to_string());
    }
    if composer_deps.iter().any(|dep| dep.starts_with("hyperf/")) {
        stack.push("Framework: Hyperf".to_string());
    }
    if has_php_framework(evidence, "dux") {
        stack.push("Framework: Dux".to_string());
    }
    if evidence.pyproject.is_some() {
        stack.push("Language: Python".to_string());
        stack.push("Project config: pyproject.toml".to_string());
    }
    if evidence
        .pyproject
        .as_deref()
        .is_some_and(|text| contains_token(text, "fastapi"))
    {
        stack.push("Framework: FastAPI".to_string());
    }
    if evidence
        .pyproject
        .as_deref()
        .is_some_and(|text| contains_token(text, "django"))
    {
        stack.push("Framework: Django".to_string());
    }
    if evidence
        .pyproject
        .as_deref()
        .is_some_and(|text| contains_token(text, "flask"))
    {
        stack.push("Framework: Flask".to_string());
    }
    if evidence.go_mod.is_some() {
        stack.push("Language: Go".to_string());
        stack.push("Module system: Go modules".to_string());
    }
    if evidence
        .go_mod
        .as_deref()
        .is_some_and(|text| text.contains("github.com/gin-gonic/gin"))
    {
        stack.push("Framework: Gin".to_string());
    }
    if evidence.pom.is_some() || evidence.gradle.is_some() {
        stack.push("Language/runtime: JVM".to_string());
    }
    if evidence
        .pom
        .as_deref()
        .or(evidence.gradle.as_deref())
        .is_some_and(|text| text.contains("spring-boot"))
    {
        stack.push("Framework: Spring Boot".to_string());
    }
    if evidence.pom.is_some() {
        stack.push("Build: Maven".to_string());
    }
    if evidence.gradle.is_some() {
        stack.push("Build: Gradle".to_string());
    }
    if evidence.gemfile.is_some() {
        stack.push("Language: Ruby".to_string());
        stack.push("Package manager: Bundler".to_string());
    }
    if evidence.dockerfile.is_some() || evidence.docker_compose.is_some() {
        stack.push("Runtime: Docker".to_string());
    }
    sorted_unique_strings(stack)
}

fn package_dependencies(package: Option<&Value>) -> Vec<String> {
    let mut deps = Vec::new();
    for key in ["dependencies", "devDependencies"] {
        if let Some(object) = package
            .and_then(|value| value.get(key))
            .and_then(Value::as_object)
        {
            deps.extend(object.keys().cloned());
        }
    }
    deps
}

fn composer_dependencies(composer: Option<&Value>) -> Vec<String> {
    let mut deps = Vec::new();
    for key in ["require", "require-dev"] {
        if let Some(object) = composer
            .and_then(|value| value.get(key))
            .and_then(Value::as_object)
        {
            deps.extend(object.keys().cloned());
        }
    }
    deps
}

fn cargo_manifest_text(evidence: &ProjectProfileEvidence) -> String {
    [
        evidence.cargo.as_deref().unwrap_or_default(),
        evidence.root_cargo.as_deref().unwrap_or_default(),
    ]
    .join("\n")
}

fn cargo_dependency_present(evidence: &ProjectProfileEvidence, dependency: &str) -> bool {
    let manifest = cargo_manifest_text(evidence);
    manifest.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with(&format!("{dependency} ="))
            || trimmed.starts_with(&format!("{dependency}="))
    })
}
