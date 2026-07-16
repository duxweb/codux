use super::*;

pub fn pet_refresh(
    service: &RuntimeService,
    _request: PetRefreshRequest,
) -> Result<PetSnapshot, String> {
    service.refresh_pet_from_indexed_history()?;
    service.pet_snapshot()
}
pub fn pet_catalog(service: &RuntimeService) -> Result<PetCatalog, String> {
    Ok(service.pet_catalog())
}
pub fn pet_snapshot(service: &RuntimeService) -> Result<PetSnapshot, String> {
    service.pet_snapshot()
}
pub fn pet_idle_speech(
    service: &RuntimeService,
    request: PetIdleSpeechRequest,
) -> Result<PetIdleSpeechResponse, String> {
    service.pet_idle_speech(request)
}
pub async fn pet_custom_install_preview(
    service: &RuntimeService,
    request: PetCustomPetInstallRequest,
) -> Result<PetCustomPetInstallPreview, String> {
    service.resolve_custom_pet_install(request).await
}
pub async fn pet_custom_install(
    service: &RuntimeService,
    request: PetCustomPetInstallRequest,
) -> Result<PetCustomPet, String> {
    service.install_custom_pet(request).await
}
pub fn pet_custom_sprite(
    service: &RuntimeService,
    pet: PetCustomPet,
) -> Result<PetCustomPet, String> {
    Ok(service.custom_pet_sprite(pet))
}
pub fn pet_claim(
    service: &RuntimeService,
    request: PetClaimRequest,
) -> Result<PetSnapshot, String> {
    service.claim_pet_from_indexed_history(request)
}
pub fn pet_rename(
    service: &RuntimeService,
    request: PetRenameRequest,
) -> Result<PetSnapshot, String> {
    service.rename_pet(request)
}
pub fn pet_archive_current(service: &RuntimeService) -> Result<PetSnapshot, String> {
    service.archive_current_pet()
}
pub fn pet_restore_archived(
    service: &RuntimeService,
    request: PetRestoreRequest,
) -> Result<PetSnapshot, String> {
    service.restore_archived_pet(request)
}
pub fn desktop_pet_start_drag() -> Result<(), String> {
    Ok(())
}
pub fn desktop_pet_show_context_menu(_service: &RuntimeService) -> Result<(), String> {
    Ok(())
}
pub fn desktop_pet_placement(
    service: &RuntimeService,
    position: DesktopPetPhysicalPosition,
    size: DesktopPetPhysicalSize,
    work_area: DesktopPetWorkArea,
) -> DesktopPetPlacementSnapshot {
    service.desktop_pet_placement(position, size, work_area)
}
pub fn desktop_pet_set_bubble_visible(
    service: &RuntimeService,
    visible: bool,
) -> DesktopPetVisibilitySnapshot {
    service.desktop_pet_set_bubble_visible(visible)
}
pub fn desktop_pet_sync_visibility(
    service: &RuntimeService,
) -> Result<DesktopPetVisibilitySnapshot, String> {
    service.desktop_pet_sync_visibility()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::PetRefreshInput;
    use uuid::Uuid;

    #[test]
    fn pet_commands_delegate_to_runtime_pet_store() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-pet-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());

        if service
            .pet_snapshot()
            .map(|snapshot| snapshot.claimed_at.is_none())
            .unwrap_or(true)
        {
            service
                .claim_pet(crate::pet::PetClaimInput {
                    species: "dragon".to_string(),
                    custom_name: " Spark ".to_string(),
                    custom_pet: None,
                    workspaces: Vec::new(),
                })
                .expect("seed claimed pet");
        }

        let catalog = pet_catalog(&service).expect("pet catalog");
        assert!(!catalog.species.is_empty());

        let snapshot = pet_snapshot(&service).expect("pet snapshot");
        assert!(snapshot.claimed_at.is_some());

        let renamed = pet_rename(
            &service,
            PetRenameRequest {
                custom_name: " Ember ".to_string(),
            },
        )
        .expect("rename pet");
        assert_eq!(renamed.custom_name, "Ember");

        assert!(
            pet_restore_archived(
                &service,
                PetRestoreRequest {
                    legacy_id: "missing".to_string(),
                },
            )
            .expect_err("missing archived pet")
            .contains("Archived pet not found")
        );

        let refreshed = service
            .refresh_pet(PetRefreshInput {
                experience_tokens: 250,
                daily_experience_tokens: 150,
                computed_stats: Default::default(),
            })
            .expect("refresh pet directly");
        assert!(refreshed.updated_at > 0);

        let archived = pet_archive_current(&service).expect("archive pet");
        assert!(archived.claimed_at.is_none());
        assert!(!archived.legacy.is_empty());

        let custom = pet_custom_sprite(
            &service,
            PetCustomPet {
                id: "demo".to_string(),
                display_name: "Demo".to_string(),
                description: String::new(),
                spritesheet_path: "sprite.png".to_string(),
                directory_name: "demo".to_string(),
                spritesheet_data_url: None,
                source_page_url: None,
                source_zip_url: None,
                installed_at: None,
            },
        )
        .expect("custom sprite hydrate");
        assert_eq!(custom.id, "demo");

        let idle = pet_idle_speech(
            &service,
            PetIdleSpeechRequest {
                event: String::new(),
                facts: "Idle test".to_string(),
            },
        )
        .expect("fallback idle speech");
        assert!(idle.text.is_empty());

        let _ = std::fs::remove_dir_all(support_dir);
    }
}
