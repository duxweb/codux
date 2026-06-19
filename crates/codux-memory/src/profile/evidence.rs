mod source;

use super::MemoryProjectProfile;
use crate::{normalized_string, now_seconds};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, path::Path};

use source::{
    ProjectSourceSample, collect_project_source_samples, render_source_sample_signals,
    source_module_names,
};

pub(super) fn build_project_profile(
    project_id: &str,
    project_name: &str,
    workspace_path: &str,
) -> Option<MemoryProjectProfile> {
    let root = Path::new(workspace_path);
    if !root.is_dir() {
        return None;
    }
    let evidence = collect_project_profile_evidence(root);
    let overview = infer_project_overview(project_name, &evidence);
    let tech_stack = infer_project_tech_stack(&evidence);
    let commands = infer_project_commands(&evidence);
    let modules = infer_project_modules(root, &evidence);
    let source_signals = render_source_sample_signals(&evidence.source_samples);
    let app_name = evidence.app_name.as_deref().unwrap_or(project_name);

    let mut sections = Vec::new();
    sections.push(format!(
        "Project: {app_name}\nOverview: {}",
        overview.replace('\n', " ")
    ));
    if !tech_stack.is_empty() {
        sections.push(format!("Tech stack:\n{}", bullet_lines(&tech_stack)));
    }
    if !commands.is_empty() {
        sections.push(format!("Common commands:\n{}", bullet_lines(&commands)));
    }
    if !evidence.directories.is_empty() {
        sections.push(format!(
            "Top-level directories:\n{}",
            bullet_lines(&evidence.directories)
        ));
    }
    if !modules.is_empty() {
        sections.push(format!("Detected modules:\n{}", bullet_lines(&modules)));
    }
    if !source_signals.is_empty() {
        sections.push(format!(
            "Source signals:\n{}",
            bullet_lines(&source_signals)
        ));
    }
    Some(MemoryProjectProfile {
        project_id: project_id.to_string(),
        content: sections.join("\n\n"),
        source_fingerprint: evidence.source_fingerprint,
        created_at: now_seconds(),
        updated_at: now_seconds(),
    })
}

#[derive(Debug, Default)]
struct ProjectProfileEvidence {
    readme: Option<String>,
    package: Option<Value>,
    composer: Option<Value>,
    cargo: Option<String>,
    root_cargo: Option<String>,
    tauri: Option<Value>,
    pyproject: Option<String>,
    go_mod: Option<String>,
    pom: Option<String>,
    gradle: Option<String>,
    gemfile: Option<String>,
    dockerfile: Option<String>,
    docker_compose: Option<String>,
    app_name: Option<String>,
    directories: Vec<String>,
    source_samples: Vec<ProjectSourceSample>,
    root_markers: HashSet<String>,
    package_manager: String,
    source_fingerprint: String,
}

fn collect_project_profile_evidence(root: &Path) -> ProjectProfileEvidence {
    let readme = read_first_existing_file(
        root,
        &["README.md", "README.zh-CN.md", "README.cn.md", "readme.md"],
        18_000,
    );
    let package = read_json_file(&root.join("package.json"));
    let composer = read_json_file(&root.join("composer.json"));
    let cargo = read_limited_file(&root.join("src-tauri").join("Cargo.toml"), 10_000);
    let root_cargo = read_limited_file(&root.join("Cargo.toml"), 10_000);
    let tauri = read_json_file(&root.join("src-tauri").join("tauri.conf.json"))
        .or_else(|| read_json_file(&root.join("src-tauri").join("tauri.conf.json5")));
    let pyproject = read_limited_file(&root.join("pyproject.toml"), 20_000);
    let go_mod = read_limited_file(&root.join("go.mod"), 12_000);
    let pom = read_limited_file(&root.join("pom.xml"), 20_000);
    let gradle = read_first_existing_file(root, &["build.gradle", "build.gradle.kts"], 20_000);
    let gemfile = read_limited_file(&root.join("Gemfile"), 12_000);
    let dockerfile = read_limited_file(&root.join("Dockerfile"), 16_000);
    let docker_compose = read_first_existing_file(
        root,
        &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ],
        20_000,
    );
    let directories = top_level_directories(root);
    let root_markers = root_file_markers(root);
    let package_manager = detect_package_manager(root);
    let source_samples = collect_project_source_samples(root);
    let app_name = infer_project_app_name(
        package.as_ref(),
        composer.as_ref(),
        tauri.as_ref(),
        pyproject.as_deref(),
        go_mod.as_deref(),
        root,
    );
    let fingerprint_input = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        readme.as_deref().unwrap_or_default(),
        package.as_ref().map(Value::to_string).unwrap_or_default(),
        composer.as_ref().map(Value::to_string).unwrap_or_default(),
        cargo.as_deref().unwrap_or_default(),
        root_cargo.as_deref().unwrap_or_default(),
        tauri.as_ref().map(Value::to_string).unwrap_or_default(),
        pyproject.as_deref().unwrap_or_default(),
        go_mod.as_deref().unwrap_or_default(),
        pom.as_deref().unwrap_or_default(),
        gradle.as_deref().unwrap_or_default(),
        gemfile.as_deref().unwrap_or_default(),
        dockerfile.as_deref().unwrap_or_default(),
        docker_compose.as_deref().unwrap_or_default(),
        render_source_sample_signals(&source_samples).join("\n"),
        package_manager,
        directories.join("|")
    );

    ProjectProfileEvidence {
        readme,
        package,
        composer,
        cargo,
        root_cargo,
        tauri,
        pyproject,
        go_mod,
        pom,
        gradle,
        gemfile,
        dockerfile,
        docker_compose,
        app_name,
        directories,
        source_samples,
        root_markers,
        package_manager: package_manager.to_string(),
        source_fingerprint: sha256_hex(&fingerprint_input),
    }
}

include!("evidence/io.rs");
include!("evidence/app_name.rs");
include!("evidence/overview.rs");
include!("evidence/stack.rs");
include!("evidence/commands.rs");
include!("evidence/modules.rs");
include!("evidence/helpers.rs");
