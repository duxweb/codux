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
            memberships: Vec::new(),
            experience_base_tokens: 1,
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
fn sanitize_state_keeps_current_membership_model() {
    let snapshot = PetSnapshot {
        state_version: STATE_VERSION,
        claimed_at: Some(10),
        current_experience_tokens: 42_000,
        experience_base_tokens: 100,
        daily_experience_tokens: 120,
        daily_experience_day: Some(day_index(now_seconds())),
        memberships: vec![PetProjectMembership {
            project_path: "/tmp/project-a".to_string(),
            included_at: 10,
            excluded_at: None,
        }],
        ..PetSnapshot::default()
    };

    let sanitized = sanitize_state(snapshot);

    assert_eq!(sanitized.state_version, STATE_VERSION);
    assert_eq!(sanitized.current_experience_tokens, 42_000);
    assert_eq!(sanitized.experience_base_tokens, 100);
    assert_eq!(sanitized.daily_experience_tokens, 120);
    assert_eq!(sanitized.memberships.len(), 1);
    assert_eq!(sanitized.memberships[0].included_at, 10);
}

#[test]
fn refresh_recomputes_experience_and_allows_downward_correction() {
    let mut snapshot = sanitize_state(PetSnapshot {
        claimed_at: Some(10),
        current_experience_tokens: 142_000,
        experience_base_tokens: 2_000,
        ..PetSnapshot::default()
    });
    refresh_state(
        &mut snapshot,
        PetRefreshInput {
            experience_tokens: 3_000,
            daily_experience_tokens: 700,
            computed_stats: PetStats::default(),
        },
    );
    assert_eq!(snapshot.current_experience_tokens, 5_000);
    assert_eq!(snapshot.daily_experience_tokens, 700);

    refresh_state(
        &mut snapshot,
        PetRefreshInput {
            experience_tokens: 1_500,
            daily_experience_tokens: 200,
            computed_stats: PetStats::default(),
        },
    );
    assert_eq!(snapshot.current_experience_tokens, 3_500);
    assert_eq!(snapshot.daily_experience_tokens, 200);
    assert_eq!(snapshot.progress.total_xp, 3_500);
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
fn local_legacy_state_preserves_pending_membership_migration() {
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
    assert_eq!(snapshot.state_version, 8);
    assert!(snapshot.memberships.is_empty());
    assert!(snapshot.experience_recalibration_pending);
    assert_eq!(snapshot.pending_project_token_watermarks.len(), 1);
    assert!(
        snapshot
            .pending_project_token_watermarks
            .contains_key("project-a")
    );
    assert_eq!(snapshot.experience_base_tokens, 0);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn pet_store_claim_refresh_rename_archive_restore_and_persist() {
    let support_dir = temp_support_dir();
    let project_path = support_dir.join("project").to_string_lossy().into_owned();
    let store = PetStore {
        state: Mutex::new(PetSnapshot::default()),
        state_file: support_dir.join("pet-state.dat"),
    };

    let claimed = store
        .claim(PetClaimInput {
            species: "dragon".to_string(),
            custom_name: " Spark ".to_string(),
            custom_pet: None,
            workspaces: vec![PetWorkspace {
                project_path: project_path.clone(),
            }],
        })
        .unwrap();
    assert_eq!(claimed.species, "dragon");
    assert_eq!(claimed.custom_name, "Spark");
    assert_eq!(claimed.memberships.len(), 1);
    assert_eq!(claimed.current_experience_tokens, 0);

    let refreshed = store
        .refresh(PetRefreshInput {
            experience_tokens: 150,
            daily_experience_tokens: 150,
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
        .restore_archived(
            PetRestoreRequest {
                legacy_id: archived.legacy[0].id.clone(),
            },
            vec![PetWorkspace {
                project_path: project_path,
            }],
        )
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
fn pet_store_instances_share_one_state_lock_per_file() {
    let support_dir = temp_support_dir();
    let first = PetStore::load_or_seed(support_dir.clone());
    let second = PetStore::load_or_seed(support_dir.clone());

    assert!(Arc::ptr_eq(&first, &second));

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn failed_pet_state_save_does_not_commit_memory_snapshot() {
    let support_dir = temp_support_dir();
    let store = PetStore {
        state: Mutex::new(PetSnapshot::default()),
        state_file: support_dir.clone(),
    };

    store
        .claim(PetClaimInput {
            species: "dragon".to_string(),
            custom_name: "Spark".to_string(),
            custom_pet: None,
            workspaces: Vec::new(),
        })
        .expect_err("writing state to a directory must fail");

    assert!(store.snapshot().unwrap().claimed_at.is_none());
    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn membership_intervals_track_add_remove_and_readd() {
    let support_dir = temp_support_dir();
    let store = PetStore {
        state: Mutex::new(PetSnapshot::default()),
        state_file: support_dir.join("pet-state.dat"),
    };

    let workspace_a = PetWorkspace {
        project_path: "/tmp/project-a".to_string(),
    };
    let workspace_b = PetWorkspace {
        project_path: "/tmp/project-b".to_string(),
    };
    let mut state = PetSnapshot {
        state_version: STATE_VERSION - 1,
        claimed_at: Some(10),
        species: "dragon".to_string(),
        ..PetSnapshot::default()
    };
    state.state_version = STATE_VERSION - 1;
    *store.state.lock().unwrap() = state;

    let migrated = store
        .sync_memberships_at(vec![workspace_a.clone()], Vec::new(), 100)
        .unwrap();
    assert_eq!(migrated.memberships[0].included_at, 10);
    assert_eq!(migrated.state_version, STATE_VERSION);
    assert!(migrated.experience_recalibration_pending);

    let calibrated = store
        .refresh(PetRefreshInput {
            experience_tokens: 50,
            daily_experience_tokens: 50,
            computed_stats: PetStats {
                wisdom: 10,
                chaos: 20,
                night: 30,
                stamina: 40,
                empathy: 50,
            },
        })
        .unwrap();
    assert_eq!(calibrated.state_version, STATE_VERSION);
    assert!(!calibrated.experience_recalibration_pending);
    assert_eq!(calibrated.current_stats.empathy, 50);

    let added = store
        .sync_memberships_at(
            vec![workspace_a.clone(), workspace_b.clone()],
            Vec::new(),
            200,
        )
        .unwrap();
    assert_eq!(added.memberships[1].included_at, 200);

    let removed = store
        .sync_memberships_at(vec![workspace_b.clone()], Vec::new(), 300)
        .unwrap();
    assert_eq!(removed.memberships[0].excluded_at, Some(300));
    assert_eq!(removed.memberships[1].excluded_at, None);

    let readded = store
        .sync_memberships_at(vec![workspace_a, workspace_b], Vec::new(), 400)
        .unwrap();
    assert_eq!(readded.memberships.len(), 3);
    assert_eq!(readded.memberships[2].included_at, 400);
    assert_eq!(readded.memberships[2].excluded_at, None);

    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn legacy_project_ids_migrate_to_path_membership_intervals_once() {
    let support_dir = temp_support_dir();
    let store = PetStore {
        state: Mutex::new(PetSnapshot {
            state_version: STATE_VERSION - 1,
            claimed_at: Some(10),
            pending_project_token_watermarks: HashMap::from([
                ("active".to_string(), 10),
                ("removed".to_string(), 20),
            ]),
            ..PetSnapshot::default()
        }),
        state_file: support_dir.join("pet-state.dat"),
    };

    let migrated = store
        .sync_memberships_at(
            vec![PetWorkspace {
                project_path: "/tmp/active".to_string(),
            }],
            vec!["/tmp/active".to_string(), "/tmp/removed".to_string()],
            100,
        )
        .unwrap();

    assert!(migrated.pending_project_token_watermarks.is_empty());
    assert!(migrated.memberships.iter().any(|membership| {
        membership.project_path.ends_with("/tmp/active")
            && membership.included_at == 10
            && membership.excluded_at.is_none()
    }));
    assert!(migrated.memberships.iter().any(|membership| {
        membership.project_path.ends_with("/tmp/removed")
            && membership.included_at == 10
            && membership.excluded_at == Some(100)
    }));
    let encoded = serde_json::to_value(&migrated).unwrap();
    assert!(encoded.get("projectNormalizedTokenWatermarks").is_none());
    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn intermediate_v11_project_id_memberships_are_rebuilt_from_paths() {
    let support_dir = temp_support_dir();
    let snapshot = serde_json::from_value::<PetSnapshot>(serde_json::json!({
        "stateVersion": 11,
        "statsModelVersion": STATS_MODEL_VERSION,
        "claimedAt": 10,
        "species": "dragon",
        "customName": "Spark",
        "currentExperienceTokens": 42,
        "currentStats": {
            "wisdom": 1,
            "chaos": 2,
            "night": 3,
            "stamina": 4,
            "empathy": 5
        },
        "statsUpdatedDay": null,
        "memberships": [{
            "projectId": "legacy-project-id",
            "includedAt": 10,
            "excludedAt": null
        }],
        "updatedAt": 10
    }))
    .unwrap();
    let store = PetStore {
        state: Mutex::new(sanitize_state(snapshot)),
        state_file: support_dir.join("pet-state.dat"),
    };

    let migrated = store
        .sync_memberships_at(
            vec![PetWorkspace {
                project_path: "/tmp/active".to_string(),
            }],
            vec!["/tmp/removed".to_string()],
            100,
        )
        .unwrap();

    assert_eq!(migrated.state_version, STATE_VERSION);
    assert!(migrated.experience_recalibration_pending);
    assert_eq!(migrated.current_experience_tokens, 42);
    assert!(
        migrated
            .memberships
            .iter()
            .all(|membership| !membership.project_path.contains("legacy-project-id"))
    );
    assert!(migrated.memberships.iter().any(|membership| {
        membership.project_path.ends_with("/tmp/active") && membership.excluded_at.is_none()
    }));
    assert!(migrated.memberships.iter().any(|membership| {
        membership.project_path.ends_with("/tmp/removed") && membership.excluded_at == Some(100)
    }));
    fs::remove_dir_all(support_dir).unwrap();
}

#[test]
fn pet_stats_use_session_shape_without_flat_placeholder_values() {
    let stats = pet_stats_from_sessions(&[crate::ai_usage_store::AIUsageIntervalSession {
        project_path: "/tmp/project-a".to_string(),
        source: "codex".to_string(),
        session_key: "session-1".to_string(),
        first_seen_at: 1_700_000_000,
        request_count: 3,
        total_tokens: 3_300,
        active_duration_seconds: 900,
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
    let stats = pet_stats_from_sessions(&[crate::ai_usage_store::AIUsageIntervalSession {
        project_path: "/tmp/project-a".to_string(),
        source: "codex".to_string(),
        session_key: "session-1".to_string(),
        first_seen_at: 1_700_000_000,
        request_count: 10,
        total_tokens: 10_000,
        active_duration_seconds: 0,
    }]);

    assert_eq!(stats.chaos, 0);
    assert_eq!(stats.stamina, 0);
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
