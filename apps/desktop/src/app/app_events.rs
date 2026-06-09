use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct PetCustomInstallEvent {
    pub(in crate::app) revision: u64,
    pub(in crate::app) custom_pet_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct PetUpdateEvent {
    pub(in crate::app) revision: u64,
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct SettingsUpdateEvent {
    pub(in crate::app) revision: u64,
    pub(in crate::app) statistics_revision: u64,
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct SshUpdateEvent {
    pub(in crate::app) revision: u64,
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct MemoryUpdateEvent {
    pub(in crate::app) revision: u64,
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct ChildWindowUpdateEvent {
    pub(in crate::app) revision: u64,
    pub(in crate::app) settings_revision: u64,
    pub(in crate::app) ssh_revision: u64,
    pub(in crate::app) memory_revision: u64,
    pub(in crate::app) project_revision: u64,
    pub(in crate::app) worktree_revision: u64,
    pub(in crate::app) git_revision: u64,
    pub(in crate::app) git_running_label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum ChildWindowUpdateKind {
    Settings,
    Ssh,
    Memory,
    Project,
    Worktree,
    Git,
}

static PET_CUSTOM_INSTALL_EVENT: OnceLock<Mutex<PetCustomInstallEvent>> = OnceLock::new();
static PET_UPDATE_EVENT: OnceLock<Mutex<PetUpdateEvent>> = OnceLock::new();
static SETTINGS_UPDATE_EVENT: OnceLock<Mutex<SettingsUpdateEvent>> = OnceLock::new();
static SSH_UPDATE_EVENT: OnceLock<Mutex<SshUpdateEvent>> = OnceLock::new();
static MEMORY_UPDATE_EVENT: OnceLock<Mutex<MemoryUpdateEvent>> = OnceLock::new();
static CHILD_WINDOW_UPDATE_EVENT: OnceLock<Mutex<ChildWindowUpdateEvent>> = OnceLock::new();

fn pet_custom_install_event() -> &'static Mutex<PetCustomInstallEvent> {
    PET_CUSTOM_INSTALL_EVENT.get_or_init(|| Mutex::new(PetCustomInstallEvent::default()))
}

pub(in crate::app) fn current_pet_custom_install_event() -> PetCustomInstallEvent {
    pet_custom_install_event()
        .lock()
        .map(|event| event.clone())
        .unwrap_or_default()
}

pub(in crate::app) fn publish_pet_custom_install(custom_pet_id: String) -> u64 {
    let Ok(mut event) = pet_custom_install_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    event.custom_pet_id = Some(custom_pet_id);
    event.revision
}

fn pet_update_event() -> &'static Mutex<PetUpdateEvent> {
    PET_UPDATE_EVENT.get_or_init(|| Mutex::new(PetUpdateEvent::default()))
}

pub(in crate::app) fn current_pet_update_event() -> PetUpdateEvent {
    pet_update_event()
        .lock()
        .map(|event| event.clone())
        .unwrap_or_default()
}

pub(in crate::app) fn publish_pet_update() -> u64 {
    let Ok(mut event) = pet_update_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    event.revision
}

fn settings_update_event() -> &'static Mutex<SettingsUpdateEvent> {
    SETTINGS_UPDATE_EVENT.get_or_init(|| Mutex::new(SettingsUpdateEvent::default()))
}

pub(in crate::app) fn current_settings_update_event() -> SettingsUpdateEvent {
    settings_update_event()
        .lock()
        .map(|event| event.clone())
        .unwrap_or_default()
}

pub(in crate::app) fn publish_settings_update() -> u64 {
    let Ok(mut event) = settings_update_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    event.revision
}

pub(in crate::app) fn publish_statistics_settings_update() -> u64 {
    let Ok(mut event) = settings_update_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    event.statistics_revision = event.revision;
    event.revision
}

fn ssh_update_event() -> &'static Mutex<SshUpdateEvent> {
    SSH_UPDATE_EVENT.get_or_init(|| Mutex::new(SshUpdateEvent::default()))
}

pub(in crate::app) fn publish_ssh_update() -> u64 {
    let Ok(mut event) = ssh_update_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    event.revision
}

fn memory_update_event() -> &'static Mutex<MemoryUpdateEvent> {
    MEMORY_UPDATE_EVENT.get_or_init(|| Mutex::new(MemoryUpdateEvent::default()))
}

pub(in crate::app) fn current_memory_update_event() -> MemoryUpdateEvent {
    memory_update_event()
        .lock()
        .map(|event| event.clone())
        .unwrap_or_default()
}

pub(in crate::app) fn publish_memory_update() -> u64 {
    let Ok(mut event) = memory_update_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    event.revision
}

fn child_window_update_event() -> &'static Mutex<ChildWindowUpdateEvent> {
    CHILD_WINDOW_UPDATE_EVENT.get_or_init(|| Mutex::new(ChildWindowUpdateEvent::default()))
}

pub(in crate::app) fn current_child_window_update_event() -> ChildWindowUpdateEvent {
    child_window_update_event()
        .lock()
        .map(|event| event.clone())
        .unwrap_or_default()
}

pub(in crate::app) fn publish_child_window_update(kind: ChildWindowUpdateKind) -> u64 {
    let Ok(mut event) = child_window_update_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    match kind {
        ChildWindowUpdateKind::Settings => {
            event.settings_revision = event.settings_revision.saturating_add(1)
        }
        ChildWindowUpdateKind::Ssh => event.ssh_revision = event.ssh_revision.saturating_add(1),
        ChildWindowUpdateKind::Memory => {
            event.memory_revision = event.memory_revision.saturating_add(1)
        }
        ChildWindowUpdateKind::Project => {
            event.project_revision = event.project_revision.saturating_add(1)
        }
        ChildWindowUpdateKind::Worktree => {
            event.worktree_revision = event.worktree_revision.saturating_add(1)
        }
        ChildWindowUpdateKind::Git => event.git_revision = event.git_revision.saturating_add(1),
    }
    event.revision
}

pub(in crate::app) fn publish_child_window_git_operation(label: Option<String>) -> u64 {
    let Ok(mut event) = child_window_update_event().lock() else {
        return 0;
    };
    event.revision = event.revision.saturating_add(1);
    event.git_revision = event.git_revision.saturating_add(1);
    event.git_running_label = label;
    event.revision
}
