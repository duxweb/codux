use crate::{
    ai_history_indexer::AIHistoryProjectState,
    ai_history_normalized::{AIGlobalHistorySnapshot, AIHistoryProjectRequest},
    ai_runtime::{
        AIRuntimeBridgeSnapshot, AIRuntimeContextSnapshot, AIRuntimeProbeRequest,
        AIRuntimeStateSnapshot,
    },
    app_info::{
        AppAboutMetadata, AppDiagnosticsSnapshot, DiagnosticsExportRequest,
        DiagnosticsExportResult, UpdateInstallResult, UpdateStatus,
    },
    desktop_pet::{
        DesktopPetPhysicalPosition, DesktopPetPhysicalSize, DesktopPetPlacementSnapshot,
        DesktopPetVisibilitySnapshot, DesktopPetWorkArea,
    },
    git::{
        GitBranchRequest, GitBranchesSnapshot, GitCloneRequest, GitCommitActionRequest,
        GitCommitMessageContextSnapshot, GitCommitRefRequest, GitCommitRequest,
        GitCreateBranchRequest, GitDeleteBranchRequest, GitDiffRequest, GitDiffSnapshot,
        GitPathsRequest, GitPushRemoteBranchRequest, GitPushRemoteRequest, GitRemoteRequest,
        GitRestoreCommitRequest, GitReviewContentRequest, GitReviewContentSnapshot,
        GitReviewDiffRequest, GitReviewSnapshot, GitStatusSnapshot, GitSummary,
        GitWatchRegistration,
    },
    llm::{
        LLMCompletionRequest, LLMCompletionResponse, LLMProviderTestResult, PetIdleSpeechRequest,
        PetIdleSpeechResponse,
    },
    memory::{
        MemoryExtractionStatusSnapshot, MemoryManagementRequest, MemoryManagementSnapshot,
        MemoryManagerSnapshot, MemoryManagerSnapshotRequest, MemoryProjectMigrationRequest,
        MemoryProjectProfileRefreshResult, MemorySummaryRow, MemorySummaryUpdateRequest,
    },
    notification::{NotificationDispatchRequest, NotificationDispatchResult},
    performance::{PerformanceMonitor, PerformanceSnapshot},
    pet::{
        PetCatalog, PetClaimRequest, PetCustomPet, PetCustomPetInstallPreview,
        PetCustomPetInstallRequest, PetRefreshRequest, PetRenameRequest, PetRestoreRequest,
        PetSnapshot,
    },
    power::PowerManager,
    project_activity::ProjectActivitySnapshot,
    project_open::{ProjectOpenApplicationRequest, ProjectOpenApplicationSummary},
    project_store::{
        ProjectCloseRequest, ProjectCreateRequest, ProjectDefaultPushRemoteRequest,
        ProjectListSnapshot, ProjectReorderRequest, ProjectSelectWorktreeRequest, ProjectSummary,
        ProjectUpdateRequest,
    },
    remote::RemoteSummary,
    runtime_state::{AppRuntimeReadySnapshot, RuntimeService, RuntimeWindowStateSnapshot},
    settings::{AIProviderSettings, AppSettings, AppSettingsStore, sync_process_locale_preference},
    ssh::SSHLaunchCommand,
    ssh::{SSHProfileTestResult, SSHProfileUpsertRequest, SSHProfilesSnapshot},
    worktree::{
        WorktreeCreateRequest, WorktreeMergeRequest, WorktreeRemoveRequest, WorktreeSnapshot,
    },
};
use std::path::PathBuf;

pub fn app_about_metadata(
    version: impl Into<String>,
    identifier: impl Into<String>,
) -> AppAboutMetadata {
    crate::app_info::about_metadata(version, identifier)
}

pub fn app_update_status(
    settings: &AppSettings,
    repo_root: PathBuf,
    current_version: &str,
) -> Result<UpdateStatus, String> {
    Ok(crate::app_info::update_status(
        settings,
        repo_root,
        current_version,
    ))
}

pub fn app_update_install(
    settings: &AppSettings,
    repo_root: PathBuf,
    current_version: &str,
) -> Result<UpdateInstallResult, String> {
    crate::app_info::install_update(settings, repo_root, current_version)
}

pub fn diagnostics_export(
    request: DiagnosticsExportRequest,
    about: AppAboutMetadata,
    update: UpdateStatus,
    snapshot: AppDiagnosticsSnapshot,
) -> Result<DiagnosticsExportResult, String> {
    crate::app_info::export_diagnostics(request, about, update, snapshot)
}

pub fn app_open_runtime_log() -> Result<(), String> {
    crate::app_info::open_runtime_log()
}

pub fn app_open_live_log() -> Result<(), String> {
    crate::app_info::open_live_log()
}

pub fn app_open_url(url: String) -> Result<(), String> {
    crate::app_info::open_url(&url)
}

pub fn app_request_restart() -> Result<(), String> {
    crate::app_info::request_restart()
}

pub fn app_toggle_devtools() -> bool {
    cfg!(debug_assertions)
}

pub fn app_window_close() -> bool {
    true
}

pub fn app_runtime_ready(
    service: &RuntimeService,
    visible: bool,
    focused: bool,
) -> AppRuntimeReadySnapshot {
    service.app_runtime_ready(visible, focused)
}

pub fn app_window_state(
    service: &RuntimeService,
    visible: bool,
    focused: bool,
) -> RuntimeWindowStateSnapshot {
    service.app_window_state(visible, focused)
}

pub fn runtime_trace_frontend(service: &RuntimeService, category: String, message: String) {
    service.runtime_trace_frontend(&category, &message);
}

pub fn localized_open_dialog(
    request: crate::dialog::LocalizedOpenDialogRequest,
) -> Result<Option<Vec<String>>, String> {
    crate::dialog::localized_open_dialog(request)
}

pub fn localized_save_dialog(
    request: crate::dialog::LocalizedSaveDialogRequest,
) -> Result<Option<String>, String> {
    crate::dialog::localized_save_dialog(request)
}

pub fn app_settings_get(store: &AppSettingsStore) -> AppSettings {
    store.snapshot()
}

pub fn app_settings_set(
    store: &AppSettingsStore,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let next = store.replace(settings)?;
    sync_process_locale_preference(&next);
    Ok(next)
}

pub fn i18n_bundle_get() -> crate::i18n::I18nBundle {
    crate::i18n::i18n_bundle()
}

pub fn performance_snapshot(monitor: &PerformanceMonitor) -> PerformanceSnapshot {
    monitor.snapshot()
}

pub fn project_mark_active(
    service: &RuntimeService,
    project: ProjectSummary,
) -> Result<ProjectActivitySnapshot, String> {
    service.mark_project_active_with_watch(&project.id)
}

pub fn project_select(
    service: &RuntimeService,
    project_id: String,
) -> Result<ProjectListSnapshot, String> {
    service.select_project(&project_id)?;
    Ok(service.project_list())
}

pub fn project_list(service: &RuntimeService) -> ProjectListSnapshot {
    service.project_list()
}

pub fn project_create(
    service: &RuntimeService,
    request: ProjectCreateRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_create(request)
}

pub fn project_update(
    service: &RuntimeService,
    request: ProjectUpdateRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_update(request)
}

pub fn project_reorder(
    service: &RuntimeService,
    request: ProjectReorderRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_reorder(request)
}

pub fn project_close(
    service: &RuntimeService,
    request: ProjectCloseRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_close(request)
}

pub fn project_select_worktree(
    service: &RuntimeService,
    request: ProjectSelectWorktreeRequest,
) -> Result<(), String> {
    service.project_select_worktree(request)
}

pub fn project_set_default_push_remote(
    service: &RuntimeService,
    request: ProjectDefaultPushRemoteRequest,
) -> Result<ProjectListSnapshot, String> {
    service.project_set_default_push_remote(request)
}

pub fn project_open_applications(service: &RuntimeService) -> Vec<ProjectOpenApplicationSummary> {
    service.project_open_applications()
}

pub fn project_open_in_application(
    service: &RuntimeService,
    request: ProjectOpenApplicationRequest,
) -> Result<(), String> {
    service.project_open_in_application(request.project_path, request.application_id)
}

pub fn project_reveal_in_file_manager(
    service: &RuntimeService,
    project_path: String,
) -> Result<(), String> {
    service.project_reveal_in_file_manager(&project_path)
}

pub fn file_watch(
    service: &RuntimeService,
    project_path: String,
) -> Result<crate::files::FileWatchRegistration, String> {
    service.file_watch(project_path)
}

pub fn file_unwatch(service: &RuntimeService, project_path: String) -> Result<(), String> {
    service.file_unwatch(project_path)
}

pub fn file_list_children(
    request: crate::files::FileChildrenRequest,
) -> Result<Vec<crate::files::FileEntry>, String> {
    crate::files::file_list_children(request)
}

pub fn file_read(
    request: crate::files::FilePathRequest,
) -> Result<crate::files::FileReadResult, String> {
    crate::files::file_read(request)
}

pub fn file_write(
    request: crate::files::FileWriteRequest,
) -> Result<crate::files::FileReadResult, String> {
    crate::files::file_write(request)
}

pub fn file_create_file(
    request: crate::files::FileCreateRequest,
) -> Result<crate::files::FileEntry, String> {
    crate::files::file_create_file(request)
}

pub fn file_create_dir(
    request: crate::files::FileCreateRequest,
) -> Result<crate::files::FileEntry, String> {
    crate::files::file_create_dir(request)
}

pub fn file_rename(
    request: crate::files::FileRenameRequest,
) -> Result<crate::files::FileEntry, String> {
    crate::files::file_rename(request)
}

pub fn file_delete(request: crate::files::FilePathRequest) -> Result<(), String> {
    crate::files::file_delete(request)
}

pub fn file_copy(
    request: crate::files::FileCopyRequest,
) -> Result<crate::files::FileEntry, String> {
    crate::files::file_copy(request)
}

pub fn file_import_external(
    request: crate::files::FileExternalCopyRequest,
) -> Result<Vec<crate::files::FileEntry>, String> {
    crate::files::file_import_external(request)
}

pub fn file_reveal(request: crate::files::FilePathRequest) -> Result<(), String> {
    crate::files::file_reveal(request)
}

pub fn file_open(request: crate::files::FilePathRequest) -> Result<(), String> {
    crate::files::file_open(request)
}

pub fn ssh_profile_upsert(
    service: &RuntimeService,
    request: SSHProfileUpsertRequest,
) -> Result<SSHProfilesSnapshot, String> {
    service.upsert_ssh_profile(request)
}

pub fn ssh_profile_delete(
    service: &RuntimeService,
    profile_id: String,
) -> Result<SSHProfilesSnapshot, String> {
    service.delete_ssh_profile(profile_id)
}

pub fn ssh_profile_test(
    service: &RuntimeService,
    request: SSHProfileUpsertRequest,
    runtime_assets: PathBuf,
) -> Result<SSHProfileTestResult, String> {
    service.test_ssh_profile(request, runtime_assets)
}

pub fn ssh_profiles(service: &RuntimeService) -> SSHProfilesSnapshot {
    service.ssh_profiles()
}

pub fn ssh_launch_command(
    service: &RuntimeService,
    profile_id: String,
) -> Result<SSHLaunchCommand, String> {
    service.ssh_launch_command(profile_id)
}

pub fn remote_status(service: &RuntimeService) -> RemoteSummary {
    service.reload_remote()
}

pub fn remote_snapshot_emit(service: &RuntimeService) -> RemoteSummary {
    service.reload_remote()
}

pub fn remote_reconnect(service: &RuntimeService) -> Result<RemoteSummary, String> {
    service.reconnect_remote()
}

pub fn remote_devices_refresh(service: &RuntimeService) -> Result<RemoteSummary, String> {
    service.refresh_remote_devices()
}

pub fn remote_device_revoke(
    service: &RuntimeService,
    device_id: String,
) -> Result<RemoteSummary, String> {
    service.revoke_remote_device(&device_id)
}

pub fn remote_pairing_create(service: &RuntimeService) -> Result<RemoteSummary, String> {
    service.create_remote_pairing()
}

pub fn remote_pairing_cancel(
    service: &RuntimeService,
    pairing_id: String,
) -> Result<RemoteSummary, String> {
    service.cancel_remote_pairing(&pairing_id)
}

pub fn remote_pairing_confirm(
    service: &RuntimeService,
    pairing_id: String,
) -> Result<RemoteSummary, String> {
    service.confirm_remote_pairing(&pairing_id)
}

pub fn remote_pairing_reject(
    service: &RuntimeService,
    pairing_id: String,
) -> Result<RemoteSummary, String> {
    service.reject_remote_pairing(&pairing_id)
}

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

pub fn worktree_create(
    service: &RuntimeService,
    request: WorktreeCreateRequest,
) -> Result<WorktreeSnapshot, String> {
    service.create_worktree_from_request(request)
}

pub fn worktree_remove(
    service: &RuntimeService,
    request: WorktreeRemoveRequest,
) -> Result<WorktreeSnapshot, String> {
    service.remove_worktree_from_request(request)
}

pub fn worktree_merge(
    service: &RuntimeService,
    request: WorktreeMergeRequest,
) -> Result<WorktreeSnapshot, String> {
    service.merge_worktree_from_request(request)
}

pub fn git_cancel(service: &RuntimeService, project_path: String) -> Result<(), String> {
    service.cancel_project_git(&project_path)
}

pub fn git_refresh_project(service: &RuntimeService, project_path: String) -> GitSummary {
    service.reload_project_git(&project_path)
}

pub fn git_status(project_path: String) -> GitStatusSnapshot {
    crate::git::git_status(project_path)
}

pub fn git_watch(
    service: &RuntimeService,
    project_path: String,
) -> Result<GitWatchRegistration, String> {
    service.git_watch(project_path)
}

pub fn git_unwatch(service: &RuntimeService, project_path: String) -> Result<(), String> {
    service.git_unwatch(project_path)
}

pub fn git_fetch(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.fetch_project_git(&project_path)
}

pub fn git_pull(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.pull_project_git(&project_path)
}

pub fn git_push(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.push_project_git(&project_path)
}

pub fn git_sync(service: &RuntimeService, project_path: String) -> Result<GitSummary, String> {
    service.sync_project_git(&project_path)
}

pub fn git_force_push(
    service: &RuntimeService,
    project_path: String,
) -> Result<GitSummary, String> {
    service.force_push_project_git(&project_path)
}

pub fn git_push_remote(
    service: &RuntimeService,
    request: GitPushRemoteRequest,
) -> Result<GitSummary, String> {
    service.push_project_git_remote(&request.project_path, &request.remote)
}

pub fn git_push_remote_branch(
    service: &RuntimeService,
    request: GitPushRemoteBranchRequest,
) -> Result<GitSummary, String> {
    service.push_project_git_remote_branch(
        &request.project_path,
        &request.remote_branch,
        request.local_branch.as_deref(),
    )
}

pub fn git_stage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_stage(request)
}

pub fn git_unstage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_unstage(request)
}

pub fn git_commit(request: GitCommitRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_commit(request)
}

pub fn git_commit_action(request: GitCommitActionRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_commit_action(request)
}

pub fn git_amend_last_commit_message(
    request: GitCommitRequest,
) -> Result<GitStatusSnapshot, String> {
    crate::git::git_amend_last_commit_message(request)
}

pub fn git_last_commit_message(project_path: String) -> Result<String, String> {
    crate::git::git_last_commit_message(project_path)
}

pub fn git_undo_last_commit(project_path: String) -> Result<GitStatusSnapshot, String> {
    crate::git::git_undo_last_commit(project_path)
}

pub fn git_head_commit_pushed(project_path: String) -> Result<bool, String> {
    crate::git::git_head_commit_pushed(project_path)
}

pub fn git_init(project_path: String) -> Result<GitStatusSnapshot, String> {
    crate::git::git_init(project_path)
}

pub fn git_clone(request: GitCloneRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_clone(request)
}

pub fn git_discard(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_discard(request)
}

pub fn git_branches(project_path: String) -> GitBranchesSnapshot {
    crate::git::git_branches(project_path)
}

pub fn git_checkout_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_checkout_branch(request)
}

pub fn git_create_branch(request: GitCreateBranchRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_create_branch(request)
}

pub fn git_checkout_remote_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_checkout_remote_branch(request)
}

pub fn git_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_merge_branch(request)
}

pub fn git_squash_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_squash_merge_branch(request)
}

pub fn git_delete_branch(request: GitDeleteBranchRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_delete_branch(request)
}

pub fn git_checkout_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_checkout_commit(request)
}

pub fn git_revert_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_revert_commit(request)
}

pub fn git_restore_commit(request: GitRestoreCommitRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_restore_commit(request)
}

pub fn git_add_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_add_remote(request)
}

pub fn git_remove_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_remove_remote(request)
}

pub fn git_append_gitignore(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    crate::git::git_append_gitignore(request)
}

pub fn git_diff_file(request: GitDiffRequest) -> GitDiffSnapshot {
    crate::git::git_diff_file(request)
}

pub fn git_commit_message_context(project_path: String) -> GitCommitMessageContextSnapshot {
    crate::git::git_commit_message_context(project_path)
}

pub fn git_review_diff_file(request: GitReviewDiffRequest) -> GitDiffSnapshot {
    crate::git::git_review_diff_file(request)
}

pub fn git_review_file_content(request: GitReviewContentRequest) -> GitReviewContentSnapshot {
    crate::git::git_review_file_content(request)
}

pub fn git_review(project_path: String, base_branch: Option<String>) -> GitReviewSnapshot {
    crate::git::git_review(project_path, base_branch)
}

pub fn ai_history_project_summary(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
) -> Result<AIHistoryProjectState, String> {
    service.indexed_project_ai_history_summary(project)
}

pub fn ai_history_refresh_project(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
) -> Result<(), String> {
    service.refresh_indexed_project_ai_history(project)
}

pub fn ai_history_project_state(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
) -> Result<AIHistoryProjectState, String> {
    service.indexed_project_ai_history_state(project)
}

pub fn ai_history_global_summary(
    service: &RuntimeService,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<AIGlobalHistorySnapshot, String> {
    service.indexed_global_ai_history_summary(projects)
}

pub fn ai_history_refresh_global(
    service: &RuntimeService,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<(), String> {
    service.refresh_indexed_global_ai_history(projects)
}

pub fn ai_history_global_state(
    service: &RuntimeService,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>, String> {
    service.indexed_global_ai_history_state(projects)
}

pub fn ai_history_global_today_normalized_tokens(service: &RuntimeService) -> Result<i64, String> {
    service.global_today_normalized_ai_tokens()
}

pub fn ai_history_session_rename(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<AIHistoryProjectState, String> {
    service.rename_indexed_ai_session(project, session_id, title)
}

pub fn ai_history_session_remove(
    service: &RuntimeService,
    project: AIHistoryProjectRequest,
    session_id: String,
) -> Result<AIHistoryProjectState, String> {
    service.remove_indexed_ai_session(project, session_id)
}

pub fn memory_extraction_cancel(
    service: &RuntimeService,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    service.cancel_memory_extraction_queue()
}

pub fn memory_extraction_status(
    service: &RuntimeService,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    service.memory_extraction_status()
}

pub fn memory_extraction_clear_failures(
    service: &RuntimeService,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    service.clear_memory_extraction_failures()
}

pub fn memory_management_snapshot(
    service: &RuntimeService,
    request: MemoryManagementRequest,
) -> Result<MemoryManagementSnapshot, String> {
    service.memory_management_snapshot(request)
}

pub fn memory_manager_snapshot(
    service: &RuntimeService,
    request: MemoryManagerSnapshotRequest,
) -> Result<MemoryManagerSnapshot, String> {
    Ok(service.memory_manager_snapshot(&service.reload_state().projects, request))
}

pub fn memory_archive_entry(service: &RuntimeService, entry_id: String) -> Result<(), String> {
    service.archive_memory_entry(None, &entry_id).map(|_| ())
}

pub fn memory_delete_entry(service: &RuntimeService, entry_id: String) -> Result<(), String> {
    service.delete_memory_entry(None, &entry_id).map(|_| ())
}

pub fn memory_delete_summary(service: &RuntimeService, summary_id: String) -> Result<(), String> {
    service.delete_memory_summary(None, &summary_id).map(|_| ())
}

pub fn memory_delete_project_profile(
    service: &RuntimeService,
    project_id: String,
) -> Result<(), String> {
    service
        .delete_memory_project_profile(&project_id)
        .map(|_| ())
}

pub fn memory_delete_project(service: &RuntimeService, project_id: String) -> Result<(), String> {
    service.delete_memory_project(&project_id).map(|_| ())
}

pub fn memory_migrate_project(
    service: &RuntimeService,
    request: MemoryProjectMigrationRequest,
) -> Result<(), String> {
    service.migrate_memory_project(request).map(|_| ())
}

pub fn memory_update_summary(
    service: &RuntimeService,
    request: MemorySummaryUpdateRequest,
) -> Result<MemorySummaryRow, String> {
    service.update_memory_summary(request)
}

pub async fn memory_index_now(
    service: &RuntimeService,
) -> Result<MemoryExtractionStatusSnapshot, String> {
    service.process_memory_sessions_now().await
}

pub async fn memory_refresh_project_profile(
    service: &RuntimeService,
    project_id: String,
) -> Result<MemoryProjectProfileRefreshResult, String> {
    service
        .force_refresh_memory_project_profile_with_llm(&project_id)
        .await
}

pub fn llm_complete(
    service: &RuntimeService,
    request: LLMCompletionRequest,
) -> Result<LLMCompletionResponse, String> {
    service.complete_llm(request)
}

pub async fn llm_provider_test(
    provider: AIProviderSettings,
) -> Result<LLMProviderTestResult, String> {
    crate::llm::test_provider(provider).await
}

pub fn ai_runtime_snapshot(service: &RuntimeService) -> AIRuntimeBridgeSnapshot {
    service.ai_runtime_bridge_snapshot()
}

pub fn ai_runtime_probe(
    service: &RuntimeService,
    request: AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    service.ai_runtime_probe(request)
}

pub fn ai_runtime_state_snapshot(service: &RuntimeService) -> AIRuntimeStateSnapshot {
    service.ai_runtime_state_snapshot()
}

pub fn ai_runtime_dismiss_completion(service: &RuntimeService, project_id: String) -> bool {
    service.ai_runtime_dismiss_completion(&project_id)
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

pub fn power_set_sleep_prevention(manager: &PowerManager, mode: String) -> Result<bool, String> {
    manager.set_sleep_prevention(mode)
}

pub fn notification_dispatch_channels(
    request: NotificationDispatchRequest,
) -> NotificationDispatchResult {
    crate::notification::dispatch_notification_channels_blocking(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::{PetProjectTokenTotal, PetRefreshInput};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn app_command_names_delegate_to_runtime_system_logic() {
        let mut settings = AppSettings::default();
        settings.update.enabled = false;
        let status = app_update_status(&settings, PathBuf::new(), "1.2.3").expect("update status");
        assert_eq!(status.current_version, "1.2.3");
        assert_eq!(status.installation_mode, "disabled");

        let about = app_about_metadata("1.2.3", "com.duxweb.codux.test");
        assert_eq!(about.version, "1.2.3");
        assert_eq!(about.identifier, "com.duxweb.codux.test");

        let manager = PowerManager::default();
        assert!(!power_set_sleep_prevention(&manager, "off".to_string()).expect("power off"));
        assert!(app_window_close());
    }

    #[test]
    fn app_lifecycle_commands_delegate_to_runtime_service() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-lifecycle-{}", Uuid::new_v4()));
        let project_dir = support_dir.join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        std::fs::write(
            support_dir.join("state.json"),
            serde_json::to_string_pretty(&json!({
                "projects": [
                    {
                        "id": "project-a",
                        "name": "Project A",
                        "path": project_dir.display().to_string()
                    }
                ],
                "selectedProjectId": "project-a"
            }))
            .expect("state json"),
        )
        .expect("write state");
        let service = RuntimeService::new(support_dir.clone());

        runtime_trace_frontend(
            &service,
            "test".to_string(),
            "lifecycle command".to_string(),
        );
        let ready = app_runtime_ready(&service, true, true);
        assert_eq!(ready.projects.projects.len(), 1);
        assert_eq!(
            ready.projects.selected_project_id.as_deref(),
            Some("project-a")
        );
        assert_eq!(
            ready.project_activity.active_project_id.as_deref(),
            Some("project-a")
        );

        let hidden = app_window_state(&service, false, false);
        assert!(!hidden.project_activity.visible);
        assert!(!hidden.project_activity.focused);

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn settings_i18n_and_performance_commands_match_tauri_facade_shape() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-settings-{}", Uuid::new_v4()));
        let store = AppSettingsStore::from_support_dir(support_dir.clone());
        let mut settings = app_settings_get(&store);
        settings.language = "en".to_string();
        settings.theme = "dark".to_string();

        let saved = app_settings_set(&store, settings).expect("save settings");
        assert_eq!(saved.language, "en");
        assert_eq!(store.reload_snapshot().theme, "dark");

        let bundle = i18n_bundle_get();
        assert_eq!(bundle.source_language, "en");
        assert!(bundle.locales.iter().any(|locale| locale == "zh-Hans"));
        assert!(bundle.locales.iter().any(|locale| locale == "en"));

        let snapshot = performance_snapshot(&PerformanceMonitor::default());
        assert!(snapshot.cpu_percent >= 0.0);
        assert!(snapshot.memory_bytes >= snapshot.memory.main_bytes);

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_commands_select_and_mark_active_project() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-projects-{}", Uuid::new_v4()));
        let first = support_dir.join("first");
        let second = support_dir.join("second");
        std::fs::create_dir_all(&first).expect("first project dir");
        std::fs::create_dir_all(&second).expect("second project dir");
        std::fs::write(
            support_dir.join("state.json"),
            serde_json::to_string_pretty(&json!({
                "projects": [
                    {
                        "id": "project-a",
                        "name": "Project A",
                        "path": first.display().to_string()
                    },
                    {
                        "id": "project-b",
                        "name": "Project B",
                        "path": second.display().to_string()
                    }
                ],
                "selectedProjectId": "project-a"
            }))
            .expect("state json"),
        )
        .expect("write state");

        let service = RuntimeService::new(support_dir.clone());
        let selected = project_select(&service, "project-b".to_string()).expect("select project");
        assert_eq!(selected.selected_project_id.as_deref(), Some("project-b"));

        let project = selected
            .projects
            .iter()
            .find(|project| project.id == "project-b")
            .expect("selected project")
            .clone();
        let activity = project_mark_active(&service, project).expect("mark active");
        assert_eq!(activity.active_project_id.as_deref(), Some("project-b"));
        assert!(
            activity
                .tracked_projects
                .iter()
                .any(|project| project.id == "project-b")
        );

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_management_commands_match_tauri_facade_shape() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-project-management-{}",
            Uuid::new_v4()
        ));
        let first = support_dir.join("first");
        let second = support_dir.join("second");
        std::fs::create_dir_all(&first).expect("first project dir");
        std::fs::create_dir_all(&second).expect("second project dir");
        let service = RuntimeService::new(support_dir.clone());

        let created = project_create(
            &service,
            ProjectCreateRequest {
                name: "First".to_string(),
                path: first.display().to_string(),
                badge_text: None,
                badge_symbol: Some("folder".to_string()),
                badge_color_hex: Some("#2F80ED".to_string()),
            },
        )
        .expect("create project");
        assert_eq!(created.projects.len(), 1);
        let first_id = created.projects[0].id.clone();

        let created = project_create(
            &service,
            ProjectCreateRequest {
                name: "Second".to_string(),
                path: second.display().to_string(),
                badge_text: None,
                badge_symbol: None,
                badge_color_hex: None,
            },
        )
        .expect("create second project");
        assert_eq!(created.projects.len(), 2);
        let second_id = created.projects[1].id.clone();

        let listed = project_list(&service);
        assert_eq!(listed.projects.len(), 2);

        let updated = project_update(
            &service,
            ProjectUpdateRequest {
                project_id: first_id.clone(),
                name: "First Renamed".to_string(),
                path: first.display().to_string(),
                badge_text: None,
                badge_symbol: Some("book".to_string()),
                badge_color_hex: Some("#78D891".to_string()),
            },
        )
        .expect("update project");
        assert!(
            updated
                .projects
                .iter()
                .any(|project| project.id == first_id && project.name == "First Renamed")
        );

        let reordered = project_reorder(
            &service,
            ProjectReorderRequest {
                project_ids: vec![second_id.clone(), first_id.clone()],
            },
        )
        .expect("reorder projects");
        assert_eq!(reordered.projects[0].id, second_id);

        let remotes = project_set_default_push_remote(
            &service,
            ProjectDefaultPushRemoteRequest {
                project_id: first_id.clone(),
                remote_name: Some("origin/main".to_string()),
            },
        )
        .expect("set default remote");
        assert_eq!(
            remotes
                .projects
                .iter()
                .find(|project| project.id == first_id)
                .and_then(|project| project.git_default_push_remote_name.as_deref()),
            Some("origin/main")
        );

        let applications = project_open_applications(&service);
        assert!(applications.iter().any(|app| app.id == "vscode"));

        let closed = project_close(
            &service,
            ProjectCloseRequest {
                project_id: first_id.clone(),
            },
        )
        .expect("close project");
        assert!(!closed.projects.iter().any(|project| project.id == first_id));

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn file_commands_delegate_to_runtime_files_layer() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-files-{}", Uuid::new_v4()));
        let project_dir = support_dir.join("project");
        let external_dir = support_dir.join("external");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        std::fs::create_dir_all(&external_dir).expect("external dir");
        let service = RuntimeService::new(support_dir.clone());

        let registration =
            file_watch(&service, project_dir.display().to_string()).expect("watch project");
        assert_eq!(
            std::path::PathBuf::from(&registration.project_path)
                .canonicalize()
                .expect("canonical registration path"),
            project_dir.canonicalize().expect("canonical project path")
        );

        let dir = file_create_dir(crate::files::FileCreateRequest {
            root_path: project_dir.display().to_string(),
            parent_path: None,
            name: "src".to_string(),
        })
        .expect("create dir");
        assert_eq!(dir.relative_path, "src");

        let file = file_create_file(crate::files::FileCreateRequest {
            root_path: project_dir.display().to_string(),
            parent_path: Some("src".to_string()),
            name: "main.rs".to_string(),
        })
        .expect("create file");
        assert_eq!(file.relative_path, "src/main.rs");

        let written = file_write(crate::files::FileWriteRequest {
            root_path: project_dir.display().to_string(),
            path: "src/main.rs".to_string(),
            content: "fn main() {}\n".to_string(),
        })
        .expect("write file");
        assert_eq!(written.content, "fn main() {}\n");

        let read = file_read(crate::files::FilePathRequest {
            root_path: project_dir.display().to_string(),
            path: "src/main.rs".to_string(),
        })
        .expect("read file");
        assert_eq!(read.name, "main.rs");

        let copied = file_copy(crate::files::FileCopyRequest {
            root_path: project_dir.display().to_string(),
            source_path: "src/main.rs".to_string(),
            target_directory_path: None,
        })
        .expect("copy file");
        assert!(copied.relative_path.starts_with("src/main copy "));
        assert!(project_dir.join(&copied.relative_path).exists());

        let renamed = file_rename(crate::files::FileRenameRequest {
            root_path: project_dir.display().to_string(),
            path: "src/main.rs".to_string(),
            new_name: "lib.rs".to_string(),
        })
        .expect("rename file");
        assert_eq!(renamed.relative_path, "src/lib.rs");

        let external_file = external_dir.join("note.txt");
        std::fs::write(&external_file, "note").expect("write external file");
        let imported = file_import_external(crate::files::FileExternalCopyRequest {
            root_path: project_dir.display().to_string(),
            source_paths: vec![external_file.display().to_string()],
            target_directory_path: Some("src".to_string()),
        })
        .expect("import external file");
        assert_eq!(imported[0].relative_path, "src/note.txt");

        let children = file_list_children(crate::files::FileChildrenRequest {
            root_path: project_dir.display().to_string(),
            directory_path: Some("src".to_string()),
        })
        .expect("list children");
        assert!(children.iter().any(|entry| entry.name == "lib.rs"));
        assert!(children.iter().any(|entry| entry.name == "note.txt"));

        file_delete(crate::files::FilePathRequest {
            root_path: project_dir.display().to_string(),
            path: "src/lib.rs".to_string(),
        })
        .expect("delete file");
        assert!(!project_dir.join("src/lib.rs").exists());

        file_unwatch(&service, project_dir.display().to_string()).expect("unwatch project");

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn ssh_profile_commands_upsert_delete_and_test_without_real_connection() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-ssh-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());
        let request = SSHProfileUpsertRequest {
            id: Some("profile-1".to_string()),
            name: "Production".to_string(),
            host: "example.com".to_string(),
            port: 2222,
            username: "root".to_string(),
            credential_kind: "password".to_string(),
            private_key_path: None,
            password: Some("secret".to_string()),
            key_passphrase: None,
        };

        let snapshot = ssh_profile_upsert(&service, request.clone()).expect("upsert profile");
        assert_eq!(snapshot.profiles.len(), 1);
        assert_eq!(snapshot.profiles[0].id, "profile-1");
        assert_eq!(snapshot.profiles[0].host, "example.com");

        let profiles = ssh_profiles(&service);
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(profiles.profiles[0].id, "profile-1");

        let launch = ssh_launch_command(&service, "profile-1".to_string()).expect("launch command");
        assert!(launch.command.contains("codux-ssh"));
        assert!(launch.command.contains("profile-1"));
        assert!(launch.log_command.contains("codux-ssh"));
        assert!(launch.log_command.contains("profile-1"));

        let test_error = ssh_profile_test(&service, request, support_dir.join("missing-bin"))
            .expect_err("missing wrapper should fail before connecting");
        assert!(test_error.contains("codux-ssh wrapper is not ready"));

        let snapshot =
            ssh_profile_delete(&service, "profile-1".to_string()).expect("delete profile");
        assert!(snapshot.profiles.is_empty());

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn remote_commands_delegate_to_runtime_service_without_network_when_disabled() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-remote-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        std::fs::write(
            support_dir.join("settings.json"),
            serde_json::to_string_pretty(&json!({
                "remote": {
                    "isEnabled": false,
                    "serverUrl": "http://relay.example"
                }
            }))
            .expect("settings json"),
        )
        .expect("write settings");

        let service = RuntimeService::new(support_dir.clone());
        let status = remote_status(&service);
        assert!(!status.enabled);
        assert_eq!(status.status, "stopped");

        let emitted = remote_snapshot_emit(&service);
        assert_eq!(emitted.status, "stopped");

        let reconnected = remote_reconnect(&service).expect("disabled reconnect");
        assert!(!reconnected.enabled);

        let refreshed = remote_devices_refresh(&service).expect("disabled refresh");
        assert_eq!(refreshed.devices, 0);

        assert!(
            remote_device_revoke(&service, String::new())
                .expect_err("missing device id")
                .contains("Missing device id")
        );
        assert!(
            remote_pairing_create(&service)
                .expect_err("disabled pairing")
                .contains("Remote Host is disabled")
        );
        assert!(
            remote_pairing_cancel(&service, String::new())
                .expect_err("missing cancel id")
                .contains("Missing pairing id")
        );
        assert!(
            remote_pairing_confirm(&service, String::new())
                .expect_err("missing confirm id")
                .contains("Missing pairing id")
        );
        assert!(
            remote_pairing_reject(&service, String::new())
                .expect_err("missing reject id")
                .contains("Missing pairing id")
        );

        let _ = std::fs::remove_dir_all(support_dir);
    }

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
                    project_totals: vec![PetProjectTokenTotal {
                        project_id: "project-a".to_string(),
                        total_tokens: 100,
                    }],
                    fallback_total_tokens: 100,
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
                project_totals: vec![PetProjectTokenTotal {
                    project_id: "project-a".to_string(),
                    total_tokens: 250,
                }],
                fallback_total_tokens: 250,
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

    #[test]
    fn worktree_commands_delegate_to_runtime_validation() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-worktree-{}", Uuid::new_v4()));
        let project_dir = support_dir.join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let service = RuntimeService::new(support_dir.clone());
        let project_path = project_dir.display().to_string();

        let create_error = worktree_create(
            &service,
            WorktreeCreateRequest {
                project_id: "project".to_string(),
                project_path: project_path.clone(),
                base_branch: None,
                branch_name: "feature/demo".to_string(),
                task_title: Some("Demo".to_string()),
            },
        )
        .expect_err("non git create should fail");
        assert!(create_error.contains("Not a Git repository"));

        let remove_error = worktree_remove(
            &service,
            WorktreeRemoveRequest {
                project_id: "project".to_string(),
                project_path: project_path.clone(),
                worktree_path: project_path.clone(),
                remove_branch: false,
            },
        )
        .expect_err("non git remove should fail");
        assert!(remove_error.contains("Not a Git repository"));

        let merge_error = worktree_merge(
            &service,
            WorktreeMergeRequest {
                project_id: "project".to_string(),
                project_path,
                worktree_path: project_dir.display().to_string(),
                base_branch: None,
                remove_branch: Some(false),
            },
        )
        .expect_err("non git merge should fail");
        assert!(merge_error.contains("Not a Git repository"));

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn git_remote_commands_delegate_to_runtime_git2_layer() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-git-{}", Uuid::new_v4()));
        let project_dir = support_dir.join("project");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let service = RuntimeService::new(support_dir.clone());
        let project_path = project_dir.display().to_string();

        git_cancel(&service, project_path.clone()).expect("cancel without token is ok");
        let snapshot = git_refresh_project(&service, project_path.clone());
        assert!(!snapshot.is_repository);
        assert!(!git_branches(project_path.clone()).is_repository);
        assert!(
            !git_diff_file(GitDiffRequest {
                project_path: project_path.clone(),
                path: "README.md".to_string(),
                staged: false,
            })
            .is_repository
        );
        assert!(!git_review(project_path.clone(), None).is_repository);
        assert!(!git_commit_message_context(project_path.clone()).is_repository);

        for result in [
            git_fetch(&service, project_path.clone()),
            git_pull(&service, project_path.clone()),
            git_push(&service, project_path.clone()),
            git_sync(&service, project_path.clone()),
            git_force_push(&service, project_path.clone()),
            git_push_remote(
                &service,
                GitPushRemoteRequest {
                    project_path: project_path.clone(),
                    remote: "origin".to_string(),
                },
            ),
            git_push_remote_branch(
                &service,
                GitPushRemoteBranchRequest {
                    project_path: project_path.clone(),
                    remote_branch: "origin/main".to_string(),
                    local_branch: Some("main".to_string()),
                },
            ),
        ] {
            assert!(result.is_err());
        }

        for result in [
            git_stage(GitPathsRequest {
                project_path: project_path.clone(),
                paths: vec!["README.md".to_string()],
            }),
            git_unstage(GitPathsRequest {
                project_path: project_path.clone(),
                paths: vec!["README.md".to_string()],
            }),
            git_commit(GitCommitRequest {
                project_path: project_path.clone(),
                message: "test".to_string(),
            }),
            git_commit_action(GitCommitActionRequest {
                project_path: project_path.clone(),
                message: "test".to_string(),
                action: "commit".to_string(),
            }),
            git_amend_last_commit_message(GitCommitRequest {
                project_path: project_path.clone(),
                message: "test".to_string(),
            }),
            git_undo_last_commit(project_path.clone()),
            git_clone(GitCloneRequest {
                project_path: project_path.clone(),
                remote_url: "https://example.invalid/repo.git".to_string(),
            }),
            git_discard(GitPathsRequest {
                project_path: project_path.clone(),
                paths: vec!["README.md".to_string()],
            }),
            git_checkout_branch(GitBranchRequest {
                project_path: project_path.clone(),
                branch: "main".to_string(),
            }),
            git_create_branch(GitCreateBranchRequest {
                project_path: project_path.clone(),
                branch: "feature".to_string(),
                from: None,
                checkout: false,
            }),
            git_checkout_remote_branch(GitBranchRequest {
                project_path: project_path.clone(),
                branch: "origin/main".to_string(),
            }),
            git_merge_branch(GitBranchRequest {
                project_path: project_path.clone(),
                branch: "main".to_string(),
            }),
            git_squash_merge_branch(GitBranchRequest {
                project_path: project_path.clone(),
                branch: "main".to_string(),
            }),
            git_delete_branch(GitDeleteBranchRequest {
                project_path: project_path.clone(),
                branch: "feature".to_string(),
                force: true,
            }),
            git_checkout_commit(GitCommitRefRequest {
                project_path: project_path.clone(),
                commit: "HEAD".to_string(),
            }),
            git_revert_commit(GitCommitRefRequest {
                project_path: project_path.clone(),
                commit: "HEAD".to_string(),
            }),
            git_restore_commit(GitRestoreCommitRequest {
                project_path: project_path.clone(),
                commit: "HEAD".to_string(),
                force_remote: false,
            }),
            git_add_remote(GitRemoteRequest {
                project_path: project_path.clone(),
                name: "origin".to_string(),
                url: Some("https://example.invalid/repo.git".to_string()),
            }),
            git_remove_remote(GitRemoteRequest {
                project_path: project_path.clone(),
                name: "origin".to_string(),
                url: None,
            }),
            git_append_gitignore(GitPathsRequest {
                project_path: project_path.clone(),
                paths: vec!["target/".to_string()],
            }),
        ] {
            assert!(result.is_err());
        }

        git_init(project_path.clone()).expect("init git repository");
        std::fs::write(project_dir.join("README.md"), "hello\n").expect("write readme");
        let diff = git_diff_file(GitDiffRequest {
            project_path: project_path.clone(),
            path: "README.md".to_string(),
            staged: false,
        });
        assert!(diff.is_repository);
        assert!(diff.diff.contains("Untracked file"));
        let staged = git_stage(GitPathsRequest {
            project_path: project_path.clone(),
            paths: vec!["README.md".to_string()],
        })
        .expect("stage readme");
        assert_eq!(staged.staged.len(), 1);
        let review = git_review(project_path.clone(), None);
        assert!(review.is_repository);
        let context = git_commit_message_context(project_path.clone());
        assert!(context.is_repository);
        let review_diff = git_review_diff_file(GitReviewDiffRequest {
            project_path: project_path.clone(),
            path: "README.md".to_string(),
            base_branch: None,
        });
        assert!(review_diff.is_repository);
        let content = git_review_file_content(GitReviewContentRequest {
            project_path: project_path.clone(),
            path: "README.md".to_string(),
            base_branch: None,
        });
        assert!(content.is_repository);
        let _ = git_head_commit_pushed(project_path).expect("head pushed status is available");

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn ai_history_commands_delegate_to_indexed_runtime_layer() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-ai-history-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());
        let project = AIHistoryProjectRequest {
            id: "project-a".to_string(),
            name: "Project A".to_string(),
            path: String::new(),
        };

        let state = ai_history_project_state(&service, project.clone()).expect("project state");
        assert_eq!(state.project_id, "project-a");
        assert_eq!(state.detail, "idle");

        let summary =
            ai_history_project_summary(&service, project.clone()).expect("project summary");
        assert_eq!(summary.project_id, "project-a");
        assert!(!summary.is_loading);

        ai_history_refresh_project(&service, project.clone()).expect("refresh project");

        let global_state =
            ai_history_global_state(&service, vec![project.clone()]).expect("global state");
        assert!(global_state.is_some());

        let global_summary =
            ai_history_global_summary(&service, Vec::new()).expect("global summary");
        assert_eq!(global_summary.project_count, 0);

        ai_history_refresh_global(&service, Vec::new()).expect("refresh global");
        assert!(ai_history_global_today_normalized_tokens(&service).expect("today tokens") >= 0);

        assert!(
            ai_history_session_rename(
                &service,
                project.clone(),
                "missing-session".to_string(),
                "Renamed".to_string(),
            )
            .expect_err("missing session rename")
            .contains("Matching session record was not found")
        );
        assert!(
            ai_history_session_remove(&service, project, "missing-session".to_string())
                .expect_err("missing session remove")
                .contains("Matching session record was not found")
        );

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn memory_commands_delegate_to_runtime_memory_store() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-memory-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());

        let status = memory_extraction_status(&service).expect("status");
        assert_eq!(status.pending_count, 0);

        let canceled = memory_extraction_cancel(&service).expect("cancel queue");
        assert_eq!(canceled.pending_count, 0);

        let manager = memory_manager_snapshot(
            &service,
            MemoryManagerSnapshotRequest {
                scope: "all".to_string(),
                project_id: None,
                tab: "active".to_string(),
                limit: Some(20),
            },
        )
        .expect("manager snapshot");
        assert!(!manager.selected_target_title.is_empty());
        assert!(manager.current_overview.active_entry_count >= 0);

        assert!(
            crate::async_runtime::block_on(memory_refresh_project_profile(
                &service,
                "missing-project".to_string(),
            ))
            .expect_err("missing project profile")
            .contains("Project not found")
        );

        assert!(
            memory_archive_entry(&service, String::new())
                .expect_err("empty archive id")
                .contains("Memory entry id is empty")
        );
        assert!(
            memory_delete_entry(&service, String::new())
                .expect_err("empty delete id")
                .contains("Memory entry id is empty")
        );
        assert!(
            memory_delete_summary(&service, String::new())
                .expect_err("empty summary id")
                .contains("Memory summary id is empty")
        );
        assert!(
            memory_delete_project_profile(&service, String::new())
                .expect_err("empty project profile id")
                .contains("Project id is empty")
        );
        assert!(
            memory_delete_project(&service, String::new())
                .expect_err("empty project id")
                .contains("Project id is empty")
        );
        assert!(
            memory_migrate_project(
                &service,
                MemoryProjectMigrationRequest {
                    from_project_id: String::new(),
                    to_project_id: "project-b".to_string(),
                    overwrite: false,
                },
            )
            .expect_err("empty migrate project id")
            .contains("project id cannot be empty")
        );
        assert!(
            memory_update_summary(
                &service,
                MemorySummaryUpdateRequest {
                    summary_id: String::new(),
                    content: String::new(),
                    max_versions: None,
                },
            )
            .expect_err("empty summary content")
            .contains("summary content cannot be empty")
        );

        let indexed =
            crate::async_runtime::block_on(memory_index_now(&service)).expect("memory index now");
        assert_eq!(indexed.running_count, 0);

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn llm_commands_delegate_to_runtime_llm_layer_without_network() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-app-command-llm-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());

        let completion_error = llm_complete(
            &service,
            LLMCompletionRequest {
                provider_id: Some("missing".to_string()),
                prompt: "Hello".to_string(),
                system_prompt: None,
                purpose: "chat".to_string(),
            },
        )
        .expect_err("missing provider");
        assert!(completion_error.contains("No available AI provider is configured"));

        let provider_error =
            crate::async_runtime::block_on(llm_provider_test(AIProviderSettings {
                id: "provider-a".to_string(),
                kind: "openAICompatible".to_string(),
                display_name: "Provider A".to_string(),
                is_enabled: true,
                model: "gpt-test".to_string(),
                base_url: "https://api.example.invalid/v1".to_string(),
                api_key: String::new(),
                use_for_memory_extraction: true,
                priority: 0,
            }))
            .expect_err("missing provider key");
        assert!(provider_error.contains("missing an API key"));

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn ai_runtime_and_desktop_pet_window_facades_are_available() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-app-command-window-runtime-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&support_dir).expect("support dir");
        let service = RuntimeService::new(support_dir.clone());

        let snapshot = ai_runtime_snapshot(&service);
        assert!(snapshot.terminals.is_empty());
        assert!(!snapshot.runtime_event_dir.is_empty());

        let runtime_state = ai_runtime_state_snapshot(&service);
        assert!(runtime_state.sessions.is_empty());
        assert_eq!(runtime_state.running_count, 0);

        let probed = ai_runtime_probe(
            &service,
            AIRuntimeProbeRequest {
                terminal_id: "terminal-a".to_string(),
                terminal_instance_id: None,
                project_id: "project-a".to_string(),
                project_path: None,
                tool: "codex".to_string(),
                external_session_id: None,
                transcript_path: None,
                started_at: None,
                updated_at: 0.0,
            },
        );
        assert!(probed.is_none());
        assert!(!ai_runtime_dismiss_completion(
            &service,
            "project-a".to_string()
        ));

        desktop_pet_start_drag().expect("drag facade");
        desktop_pet_show_context_menu(&service).expect("context menu facade");
        let placement = desktop_pet_placement(
            &service,
            DesktopPetPhysicalPosition { x: 900.0, y: 0.0 },
            DesktopPetPhysicalSize {
                width: 352.0,
                height: 202.0,
            },
            DesktopPetWorkArea {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 800.0,
                scale_factor: 1.0,
            },
        );
        assert_eq!(placement.side, "left");
        let visible = desktop_pet_set_bubble_visible(&service, true);
        assert!(visible.bubble_visible);
        let synced = desktop_pet_sync_visibility(&service).expect("sync visibility");
        assert!(synced.bubble_visible);

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn diagnostics_export_command_writes_redacted_runtime_report() {
        let destination = std::env::temp_dir().join(format!(
            "codux-app-command-diagnostics-{}.json",
            Uuid::new_v4()
        ));
        let request = DiagnosticsExportRequest {
            destination_path: destination.display().to_string(),
        };
        let about = app_about_metadata("1.0.0", "com.duxweb.codux.test");
        let update = UpdateStatus {
            current_version: "1.0.0".to_string(),
            installation_mode: "disabled".to_string(),
            message: "disabled".to_string(),
            ..Default::default()
        };
        let result = diagnostics_export(
            request,
            about,
            update,
            AppDiagnosticsSnapshot {
                settings: json!({
                    "ai": {
                        "providers": [
                            {
                                "apiKey": "secret",
                                "api_key": "secret",
                                "token": "secret"
                            }
                        ]
                    }
                }),
                projects: json!([]),
                ai_state: json!({}),
                performance: json!({}),
                ssh: json!({
                    "profiles": [
                        {
                            "password": "secret",
                            "privateKeyPath": "/tmp/key"
                        }
                    ]
                }),
            },
        )
        .expect("export diagnostics");

        assert!(result.bytes > 0);
        let content = std::fs::read_to_string(&destination).expect("diagnostics file");
        assert!(content.contains("\"apiKey\": \"******\""));
        assert!(content.contains("\"api_key\": \"******\""));
        assert!(content.contains("\"password\": \"******\""));
        assert!(content.contains("\"privateKeyPath\": \"******\""));
        let _ = std::fs::remove_file(destination);
    }
}
