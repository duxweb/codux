use super::*;
use std::fs;
use uuid::Uuid;

fn temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("codux-profile-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn project_profile_is_generated_from_repository_files() {
    let root = temp_dir();
    fs::create_dir_all(root.join("src-tauri/src")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"name":"codux-ui","description":"Codux desktop shell","scripts":{"dev":"vite","build":"vite build"},"dependencies":{"@tauri-apps/api":"latest","react":"latest"},"devDependencies":{"typescript":"latest","vite":"latest"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("src-tauri/src/lib.rs"),
        "#[tauri::command]\nasync fn memory_refresh_project_profile() {}\n",
    )
    .unwrap();
    fs::write(root.join("src/main.tsx"), "import React from 'react';\n").unwrap();

    let profile = build_project_profile("project-1", "Codux", root.to_str().unwrap()).unwrap();

    assert!(profile.content.contains("Project: codux-ui"));
    assert!(profile.content.contains("Frontend: React"));
    assert!(profile.content.contains("Desktop: Tauri"));
    assert!(profile.content.contains("npm dev"));
    assert!(
        profile
            .content
            .contains("tauri command async fn memory_refresh_project_profile")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_profile_infers_php_composer_without_readme() {
    let root = temp_dir();
    fs::create_dir_all(root.join("app/Http/Controllers")).unwrap();
    fs::write(
        root.join("composer.json"),
        r#"{"name":"duxweb/admin","require":{"duxweb/dux":"^1.0"}}"#,
    )
    .unwrap();

    let profile =
        build_project_profile("project-php", "Dux Admin", root.to_str().unwrap()).unwrap();

    assert!(profile.content.contains("Framework: Dux"));
    assert!(profile.content.contains("Language: PHP"));
    assert!(profile.content.contains("composer install"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_profile_llm_cache_fingerprint_preserves_cached_profile() {
    let raw = "repo-fingerprint";
    let llm = llm_project_profile_fingerprint(raw);

    assert!(project_profile_fingerprints_match(&llm, raw));
    assert!(project_profile_fingerprints_match(&llm, &llm));
    assert!(!project_profile_fingerprints_match(&llm, "changed"));
}

#[test]
fn project_profile_llm_response_decodes_structured_profile() {
    let decoded = decode_project_profile_llm_response_detailed(
        r#"{"project":"Codux","overview":"Rust GPUI shell","tech_stack":["Rust","GPUI"],"common_commands":["cargo check"],"top_level_directories":["src"],"detected_modules":["runtime"]}"#,
    )
    .unwrap();

    assert!(decoded.contains("Project: Codux"));
    assert!(decoded.contains("Overview: Rust GPUI shell"));
    assert!(decoded.contains("- cargo check"));
}

#[test]
fn project_profile_for_launch_stores_profile() {
    let root = temp_dir();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"codux-runtime\"\n",
    )
    .unwrap();
    let support_dir = temp_dir();
    let service = MemoryService::new(support_dir.clone());

    let profile = service
        .project_profile_for_launch("project-a", "Codux", root.to_str().unwrap())
        .unwrap();
    let stored = service
        .current_project_profile("project-a")
        .unwrap()
        .unwrap();

    assert_eq!(stored.source_fingerprint, profile.source_fingerprint);
    assert!(stored.content.contains("Native/runtime: Rust"));

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(support_dir).unwrap();
}
