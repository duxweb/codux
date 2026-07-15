use super::*;
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use serde_json::Value;
use uuid::Uuid;

fn temp_support_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codux-gpui-pet-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn encrypt_for_test(snapshot: &PetSnapshot) -> Vec<u8> {
    let json = serde_json::to_vec(snapshot).unwrap();
    let key = pet_state_cipher_key("codux");
    let cipher = Aes256Gcm::new(&key);
    let nonce = [7_u8; 12];
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), json.as_slice())
        .unwrap();
    [nonce.to_vec(), encrypted].concat()
}

#[test]
fn reads_encrypted_pet_state_summary() {
    let support_dir = temp_support_dir();
    fs::create_dir_all(support_dir.join("custom-pets/demo")).unwrap();
    fs::write(
        support_dir.join("custom-pets/demo/pet.json"),
        r#"{"id":"demo","displayName":"Demo","spritesheetPath":"sprite.png"}"#,
    )
    .unwrap();
    fs::write(
        support_dir.join("custom-pets/demo/sprite.png"),
        [1_u8, 2, 3],
    )
    .unwrap();
    let snapshot = PetSnapshot {
        claimed_at: Some(10),
        species: "rusthound".to_string(),
        custom_name: "Ferris".to_string(),
        current_experience_tokens: 4_000_000,
        daily_experience_tokens: 120,
        daily_experience_day: Some(day_index(now_seconds())),
        legacy: vec![PetLegacyRecord {
            id: "old".to_string(),
            species: "voidcat".to_string(),
            custom_pet: None,
            custom_name: String::new(),
            total_xp: 1,
            stats: PetStats::default(),
            persona_id: default_persona_id(),
            progress: PetProgressInfo::default(),
            retired_at: 1,
        }],
        ..PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.dat"),
        encrypt_for_test(&snapshot),
    )
    .unwrap();

    let summary = PetService::new(support_dir.clone()).summary();

    assert!(summary.available);
    assert!(summary.claimed);
    assert_eq!(summary.species, "rusthound");
    assert_eq!(summary.display_name, "Ferris");
    assert_eq!(summary.archived_count, 1);
    assert_eq!(summary.custom_pet_count, 1);
    assert_eq!(summary.daily_xp, 120);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn summary_resets_daily_xp_after_local_day_changes() {
    let support_dir = temp_support_dir();
    let yesterday = Local
        .with_ymd_and_hms(2026, 5, 22, 12, 0, 0)
        .single()
        .unwrap()
        .timestamp();
    let snapshot = PetSnapshot {
        claimed_at: Some(10),
        daily_experience_tokens: 120,
        daily_experience_day: Some(day_index(yesterday)),
        ..PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.dat"),
        encrypt_for_test(&snapshot),
    )
    .unwrap();

    let summary = PetService::new(support_dir.clone()).summary();

    assert_eq!(summary.daily_xp, 0);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn sanitize_state_keeps_pet_xp_and_baseline_without_version_bump() {
    let mut snapshot = PetSnapshot {
        state_version: STATE_VERSION,
        claimed_at: Some(10),
        current_experience_tokens: 42_000,
        daily_experience_tokens: 120,
        global_normalized_total_watermark: Some(99_000),
        ..PetSnapshot::default()
    };
    snapshot
        .project_normalized_token_watermarks
        .insert("project-a".to_string(), 99_000);

    let sanitized = sanitize_state(snapshot);

    assert_eq!(sanitized.state_version, STATE_VERSION);
    assert_eq!(sanitized.current_experience_tokens, 42_000);
    assert_eq!(sanitized.daily_experience_tokens, 120);
    assert_eq!(sanitized.global_normalized_total_watermark, Some(99_000));
    assert_eq!(
        sanitized
            .project_normalized_token_watermarks
            .get("project-a"),
        Some(&99_000)
    );
}

#[test]
fn history_correction_rebuilds_pet_watermarks_without_resetting_xp() {
    let mut snapshot = PetSnapshot {
        state_version: STATE_VERSION - 1,
        claimed_at: Some(10),
        current_experience_tokens: 42_000,
        daily_experience_tokens: 120,
        global_normalized_total_watermark: Some(99_000),
        total_normalized_tokens: 99_000,
        ..PetSnapshot::default()
    };
    snapshot
        .project_normalized_token_watermarks
        .insert("project-a".to_string(), 99_000);

    let mut sanitized = sanitize_state(snapshot);

    assert_eq!(sanitized.current_experience_tokens, 42_000);
    assert_eq!(sanitized.daily_experience_tokens, 120);
    assert_eq!(sanitized.progress.total_xp, 42_000);
    assert_eq!(sanitized.current_stats, PetStats::default());
    assert_eq!(sanitized.stats_updated_day, None);
    assert_eq!(sanitized.global_normalized_total_watermark, None);
    assert!(sanitized.project_normalized_token_watermarks.is_empty());
    refresh_state(
        &mut sanitized,
        PetRefreshInput {
            project_totals: vec![PetProjectTokenTotal {
                project_id: "project-a".to_string(),
                total_tokens: 1_000,
            }],
            fallback_total_tokens: 1_000,
            computed_stats: PetStats::default(),
        },
    );
    assert_eq!(sanitized.current_experience_tokens, 42_000);
    assert_eq!(
        sanitized
            .project_normalized_token_watermarks
            .get("project-a"),
        Some(&1_000)
    );
}

#[test]
fn falls_back_to_plain_json_pet_state() {
    let support_dir = temp_support_dir();
    let snapshot = PetSnapshot {
        claimed_at: Some(10),
        species: "dragon".to_string(),
        ..PetSnapshot::default()
    };
    fs::write(
        support_dir.join("pet-state.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();

    let summary = PetService::new(support_dir.clone()).summary();

    assert!(summary.available);
    assert_eq!(summary.species, "dragon");
    assert_eq!(summary.display_name, "dragon");

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn catalog_loads_custom_pets_with_data_urls() {
    let support_dir = temp_support_dir();
    fs::create_dir_all(support_dir.join("custom-pets/demo")).unwrap();
    fs::write(
            support_dir.join("custom-pets/demo/pet.json"),
            r#"{"id":"demo","displayName":"Demo","description":"Local pet","spritesheetPath":"sprite.png"}"#,
        )
        .unwrap();
    fs::write(
        support_dir.join("custom-pets/demo/sprite.png"),
        [1_u8, 2, 3],
    )
    .unwrap();

    let catalog = PetService::new(support_dir.clone()).catalog();

    assert_eq!(catalog.species.len(), PET_SPECIES.len());
    assert_eq!(catalog.atlas.columns, 8);
    assert_eq!(catalog.custom_pets.len(), 1);
    assert_eq!(catalog.custom_pets[0].id, "demo");
    assert!(
        catalog.custom_pets[0]
            .spritesheet_data_url
            .as_deref()
            .unwrap_or_default()
            .starts_with("data:image/png;base64,")
    );

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn bundled_catalog_skips_custom_pet_io() {
    let support_dir = temp_support_dir();
    fs::create_dir_all(support_dir.join("custom-pets/demo")).unwrap();
    fs::write(
        support_dir.join("custom-pets/demo/pet.json"),
        r#"{"id":"demo","displayName":"Demo","spritesheetPath":"sprite.png"}"#,
    )
    .unwrap();
    fs::write(
        support_dir.join("custom-pets/demo/sprite.png"),
        [1_u8, 2, 3],
    )
    .unwrap();

    let catalog = PetService::new(support_dir.clone()).bundled_catalog();

    assert_eq!(catalog.species.len(), PET_SPECIES.len());
    assert_eq!(catalog.atlas.columns, 8);
    assert!(catalog.custom_pets.is_empty());

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn local_pet_state_reads_legacy_hatch_tokens_as_xp() {
    let support_dir = temp_support_dir();
    fs::write(
        support_dir.join("pet-state.json"),
        r#"{
          "stateVersion": 8,
          "statsModelVersion": 3,
          "claimedAt": 10,
          "species": "dragon",
          "customName": "Spark",
          "currentHatchTokens": 42000000,
          "currentStats": {"wisdom": 1, "chaos": 2, "night": 3, "stamina": 4, "empathy": 5},
          "globalNormalizedTotalWatermark": 99000000,
          "projectNormalizedTokenWatermarks": {"project-a": 99000000},
          "totalNormalizedTokens": 99000000,
          "updatedAt": 10
        }"#,
    )
    .unwrap();

    let (snapshot, _) = PetService::new(support_dir.clone())
        .load_snapshot()
        .unwrap();
    let snapshot = sanitize_state(snapshot);

    assert_eq!(snapshot.current_experience_tokens, 42_000_000);
    assert_eq!(snapshot.progress.total_xp, 42_000_000);
    assert!(snapshot.progress.level > 1);
    assert_eq!(snapshot.global_normalized_total_watermark, None);
    assert!(snapshot.project_normalized_token_watermarks.is_empty());
    assert_eq!(snapshot.total_normalized_tokens, 0);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn pet_store_claim_refresh_rename_archive_restore_and_persist() {
    let support_dir = temp_support_dir();
    let store = PetStore {
        state: Mutex::new(PetSnapshot::default()),
        state_file: support_dir.join("pet-state.dat"),
    };

    let claimed = store
        .claim(PetClaimInput {
            species: "dragon".to_string(),
            custom_name: " Spark ".to_string(),
            custom_pet: None,
            project_totals: vec![PetProjectTokenTotal {
                project_id: "project-a".to_string(),
                total_tokens: 100,
            }],
            fallback_total_tokens: 100,
        })
        .unwrap();
    assert_eq!(claimed.species, "dragon");
    assert_eq!(claimed.custom_name, "Spark");
    assert_eq!(
        claimed.project_normalized_token_watermarks.get("project-a"),
        Some(&100)
    );

    let refreshed = store
        .refresh(PetRefreshInput {
            project_totals: vec![PetProjectTokenTotal {
                project_id: "project-a".to_string(),
                total_tokens: 250,
            }],
            fallback_total_tokens: 250,
            computed_stats: PetStats {
                wisdom: 90,
                chaos: 10,
                night: 0,
                stamina: 0,
                empathy: 0,
            },
        })
        .unwrap();
    assert_eq!(refreshed.current_experience_tokens, 150);
    assert_eq!(refreshed.daily_experience_tokens, 150);
    assert_eq!(refreshed.persona_id, "philosopher");

    let renamed = store
        .rename(PetRenameRequest {
            custom_name: " Ember ".to_string(),
        })
        .unwrap();
    assert_eq!(renamed.custom_name, "Ember");

    let archived = store.archive_current().unwrap();
    assert!(archived.claimed_at.is_none());
    assert_eq!(archived.legacy.len(), 1);

    let restored = store
        .restore_archived(PetRestoreRequest {
            legacy_id: archived.legacy[0].id.clone(),
        })
        .unwrap();
    assert_eq!(restored.species, "dragon");
    assert_eq!(restored.custom_name, "Ember");
    assert_eq!(restored.current_experience_tokens, 150);

    let reloaded = PetStore::load_or_seed(support_dir.clone())
        .snapshot()
        .unwrap();
    assert_eq!(reloaded.species, "dragon");
    assert_eq!(reloaded.custom_name, "Ember");
    assert_eq!(reloaded.current_experience_tokens, 150);
    assert!(
        serde_json::from_slice::<Value>(&fs::read(support_dir.join("pet-state.dat")).unwrap())
            .is_err()
    );

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn adding_or_removing_projects_does_not_backfill_pet_xp() {
    let support_dir = temp_support_dir();
    let store = PetStore {
        state: Mutex::new(PetSnapshot::default()),
        state_file: support_dir.join("pet-state.dat"),
    };

    store
        .claim(PetClaimInput {
            species: "dragon".to_string(),
            custom_name: String::new(),
            custom_pet: None,
            project_totals: vec![PetProjectTokenTotal {
                project_id: "project-a".to_string(),
                total_tokens: 100,
            }],
            fallback_total_tokens: 100,
        })
        .unwrap();

    let with_added_project = store
        .refresh(PetRefreshInput {
            project_totals: vec![
                PetProjectTokenTotal {
                    project_id: "project-a".to_string(),
                    total_tokens: 120,
                },
                PetProjectTokenTotal {
                    project_id: "project-b".to_string(),
                    total_tokens: 10_000,
                },
            ],
            fallback_total_tokens: 10_120,
            computed_stats: PetStats::default(),
        })
        .unwrap();
    assert_eq!(with_added_project.current_experience_tokens, 20);
    assert_eq!(with_added_project.daily_experience_tokens, 20);

    let with_removed_project = store
        .refresh(PetRefreshInput {
            project_totals: vec![PetProjectTokenTotal {
                project_id: "project-b".to_string(),
                total_tokens: 10_010,
            }],
            fallback_total_tokens: 10_010,
            computed_stats: PetStats::default(),
        })
        .unwrap();
    assert_eq!(with_removed_project.current_experience_tokens, 30);
    assert_eq!(with_removed_project.daily_experience_tokens, 30);
    assert_eq!(
        with_removed_project
            .project_normalized_token_watermarks
            .get("project-a"),
        Some(&120)
    );

    let with_restored_project = store
        .refresh(PetRefreshInput {
            project_totals: vec![
                PetProjectTokenTotal {
                    project_id: "project-a".to_string(),
                    total_tokens: 125,
                },
                PetProjectTokenTotal {
                    project_id: "project-b".to_string(),
                    total_tokens: 10_015,
                },
            ],
            fallback_total_tokens: 10_140,
            computed_stats: PetStats::default(),
        })
        .unwrap();
    assert_eq!(with_restored_project.current_experience_tokens, 40);
    assert_eq!(with_restored_project.daily_experience_tokens, 40);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn pet_stats_use_session_shape_without_flat_placeholder_values() {
    let stats = pet_stats_from_sessions(&[crate::ai_history_normalized::AISessionSummary {
        session_id: "session-1".to_string(),
        external_session_id: None,
        project_id: "project-a".to_string(),
        project_name: "Project".to_string(),
        project_path: "/tmp/project".to_string(),
        session_title: "Short focused session".to_string(),
        first_seen_at: 1_700_000_000.0,
        last_seen_at: 1_700_000_900.0,
        last_tool: Some("codex".to_string()),
        last_model: Some("model".to_string()),
        request_count: 3,
        total_input_tokens: 2_400,
        total_output_tokens: 900,
        total_tokens: 3_300,
        cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        active_duration_seconds: 900,
        today_tokens: 3_300,
        today_cached_input_tokens: 0,
        today_usage_amounts: Vec::new(),
    }]);

    let max_trait = [
        stats.wisdom,
        stats.chaos,
        stats.night,
        stats.stamina,
        stats.empathy,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    assert!(max_trait > 0);
    assert_ne!(
        [
            stats.wisdom,
            stats.chaos,
            stats.night,
            stats.stamina,
            stats.empathy
        ],
        [100, 100, 100, 100, 100]
    );
}

#[test]
fn pet_stats_ignore_unmeasured_wall_clock_gaps() {
    let stats = pet_stats_from_sessions(&[crate::ai_history_normalized::AISessionSummary {
        session_id: "session-1".to_string(),
        external_session_id: Some("restored-session".to_string()),
        project_id: "project-a".to_string(),
        project_name: "Project".to_string(),
        project_path: "/tmp/project".to_string(),
        session_title: "Restored session".to_string(),
        first_seen_at: 1_700_000_000.0,
        last_seen_at: 1_700_604_800.0,
        last_tool: Some("codex".to_string()),
        last_model: Some("model".to_string()),
        request_count: 10,
        total_input_tokens: 8_000,
        total_output_tokens: 2_000,
        total_tokens: 10_000,
        cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        active_duration_seconds: 0,
        today_tokens: 0,
        today_cached_input_tokens: 0,
        today_usage_amounts: Vec::new(),
    }]);

    assert_eq!(stats.chaos, 0);
    assert_eq!(stats.stamina, 0);
}

#[test]
fn refresh_uses_all_time_project_watermarks_after_claim() {
    let support_dir = temp_support_dir();
    let store = PetStore {
        state: Mutex::new(PetSnapshot::default()),
        state_file: support_dir.join("pet-state.dat"),
    };

    store
        .claim(PetClaimInput {
            species: "dragon".to_string(),
            custom_name: String::new(),
            custom_pet: None,
            project_totals: vec![PetProjectTokenTotal {
                project_id: "project-a".to_string(),
                total_tokens: 100,
            }],
            fallback_total_tokens: 100,
        })
        .unwrap();

    let refreshed = store
        .refresh(PetRefreshInput {
            project_totals: vec![PetProjectTokenTotal {
                project_id: "project-a".to_string(),
                total_tokens: 130,
            }],
            fallback_total_tokens: 130,
            computed_stats: PetStats::default(),
        })
        .unwrap();

    assert_eq!(refreshed.current_experience_tokens, 30);
    assert_eq!(refreshed.daily_experience_tokens, 30);
    assert_eq!(
        refreshed
            .project_normalized_token_watermarks
            .get("project-a"),
        Some(&130)
    );

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn saturated_heavy_user_profile_gets_signature_persona_not_balanced() {
    // Regression: a long-term heavy user saturates several axes at once; the
    // old top-vs-second gate read that as "balanced" forever. Top-vs-mean must
    // surface the leaning axis instead.
    let stats = PetStats {
        wisdom: 250,
        night: 245,
        stamina: 200,
        chaos: 180,
        empathy: 150,
    };
    assert_eq!(pet_persona_id(&stats), "wise_type");
}

#[test]
fn genuinely_flat_profile_stays_balanced() {
    let stats = PetStats {
        wisdom: 200,
        chaos: 195,
        night: 205,
        stamina: 198,
        empathy: 202,
    };
    assert_eq!(pet_persona_id(&stats), "balanced");
}
