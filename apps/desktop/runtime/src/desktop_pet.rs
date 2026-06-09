use crate::{
    pet::PetStore,
    settings::{AppSettings, AppSettingsStore},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

pub const DESKTOP_PET_LABEL: &str = "desktop-pet";
pub const DESKTOP_PET_BASE_WIDTH: f64 = 352.0;
pub const DESKTOP_PET_BASE_HEIGHT: f64 = 202.0;
pub const DESKTOP_PET_MUTE_30_MINUTES: &str = "desktop-pet:mute-30-minutes";
pub const DESKTOP_PET_MUTE_1_HOUR: &str = "desktop-pet:mute-1-hour";
pub const DESKTOP_PET_MUTE_TODAY: &str = "desktop-pet:mute-today";
pub const DESKTOP_PET_SKIP_LINE: &str = "desktop-pet:skip-line";
pub const DESKTOP_PET_SPEAK_MORE: &str = "desktop-pet:speak-more";
pub const DESKTOP_PET_SPEAK_LESS: &str = "desktop-pet:speak-less";
pub const DESKTOP_PET_HIDE: &str = "desktop-pet:hide";

const DESKTOP_PET_SPRITE_SIZE: f64 = 112.0;
const DESKTOP_PET_BUBBLE_WIDTH: f64 = 198.0;
const DESKTOP_PET_BUBBLE_HEIGHT: f64 = 78.0;
const DESKTOP_PET_BUBBLE_TOP: f64 = 52.0;
const DESKTOP_PET_SPRITE_VISIBLE_INSET_X: f64 = 18.0;
const DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP: f64 = 12.0;
const DESKTOP_PET_SPRITE_VISIBLE_INSET_BOTTOM: f64 = 4.0;
const DESKTOP_PET_MARGIN: f64 = 24.0;
const DESKTOP_PET_DEFAULT_BOTTOM_MARGIN: f64 = 110.0;
static DESKTOP_PET_BUBBLE_VISIBLE: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();

fn desktop_pet_bubble_visible() -> &'static Mutex<HashMap<PathBuf, bool>> {
    DESKTOP_PET_BUBBLE_VISIBLE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPetPlacementSnapshot {
    pub side: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPetSavedOrigin {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPetPhysicalPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPetPhysicalSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPetWorkArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPetHitLayout {
    pub position: DesktopPetPhysicalPosition,
    pub size: DesktopPetPhysicalSize,
    pub scale_factor: f64,
    pub side: DesktopPetSide,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPetVisibilitySnapshot {
    pub should_show: bool,
    pub bubble_visible: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopPetSide {
    Left,
    Right,
}

impl DesktopPetSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

pub struct DesktopPetService {
    support_dir: PathBuf,
}

impl DesktopPetService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self { support_dir }
    }

    pub fn placement_file_path(&self) -> PathBuf {
        self.support_dir.join("desktop-pet-placement.json")
    }

    pub fn saved_origin(&self) -> Option<DesktopPetSavedOrigin> {
        let value = crate::config::ConfigStore::for_file(self.placement_file_path()).snapshot();
        let origin = DesktopPetSavedOrigin {
            x: value.get("x").and_then(|value| value.as_f64())?,
            y: value.get("y").and_then(|value| value.as_f64())?,
        };
        valid_saved_origin(origin)
    }

    pub fn save_origin(&self, origin: DesktopPetSavedOrigin) -> Result<(), String> {
        let origin = valid_saved_origin(origin)
            .ok_or_else(|| "Desktop pet origin must contain finite coordinates.".to_string())?;
        crate::config::ConfigStore::for_file(self.placement_file_path()).update(|snapshot| {
            snapshot.insert("x".to_string(), json!(origin.x));
            snapshot.insert("y".to_string(), json!(origin.y));
            Ok(())
        })
    }

    pub fn initial_position(&self, work_area: DesktopPetWorkArea) -> DesktopPetSavedOrigin {
        desktop_pet_initial_position(self.saved_origin(), work_area)
    }

    pub fn should_show(&self) -> Result<bool, String> {
        let settings = AppSettingsStore::from_support_dir(self.support_dir.clone()).snapshot();
        let pet = PetStore::load_or_seed(self.support_dir.clone()).snapshot()?;
        Ok(settings.pet.enabled && settings.pet.desktop_widget && pet.claimed_at.is_some())
    }

    pub fn set_bubble_visible(&self, visible: bool) -> DesktopPetVisibilitySnapshot {
        if let Ok(mut state) = desktop_pet_bubble_visible().lock() {
            state.insert(self.support_dir.clone(), visible);
        }
        DesktopPetVisibilitySnapshot {
            should_show: self.should_show().unwrap_or(false),
            bubble_visible: visible,
        }
    }

    pub fn bubble_visible(&self) -> bool {
        desktop_pet_bubble_visible()
            .lock()
            .ok()
            .and_then(|state| state.get(&self.support_dir).copied())
            .unwrap_or(false)
    }

    pub fn sync_visibility(&self) -> Result<DesktopPetVisibilitySnapshot, String> {
        let should_show = self.should_show()?;
        Ok(DesktopPetVisibilitySnapshot {
            should_show,
            bubble_visible: self.bubble_visible(),
        })
    }

    pub fn apply_menu_action(&self, action_id: &str) -> Result<AppSettings, String> {
        apply_desktop_pet_menu_action(self.support_dir.clone(), action_id)
    }
}

pub fn desktop_pet_initial_position(
    saved_origin: Option<DesktopPetSavedOrigin>,
    work_area: DesktopPetWorkArea,
) -> DesktopPetSavedOrigin {
    let scale_factor = normalized_scale_factor(work_area.scale_factor);
    let position = saved_origin.unwrap_or_else(|| DesktopPetSavedOrigin {
        x: work_area.x / scale_factor + work_area.width / scale_factor
            - DESKTOP_PET_BASE_WIDTH
            - DESKTOP_PET_MARGIN,
        y: work_area.y / scale_factor + work_area.height / scale_factor
            - DESKTOP_PET_BASE_HEIGHT
            - DESKTOP_PET_DEFAULT_BOTTOM_MARGIN,
    });
    desktop_pet_clamped_logical_position(
        position,
        DESKTOP_PET_BASE_WIDTH,
        DESKTOP_PET_BASE_HEIGHT,
        work_area,
    )
}

pub fn desktop_pet_clamped_logical_position(
    position: DesktopPetSavedOrigin,
    width: f64,
    height: f64,
    work_area: DesktopPetWorkArea,
) -> DesktopPetSavedOrigin {
    let scale_factor = normalized_scale_factor(work_area.scale_factor);
    let min_x = work_area.x / scale_factor;
    let min_y = work_area.y / scale_factor;
    let max_x = (min_x + work_area.width / scale_factor - width).max(min_x);
    let max_y = (min_y + work_area.height / scale_factor - height).max(min_y);
    DesktopPetSavedOrigin {
        x: position.x.clamp(min_x, max_x),
        y: position.y.clamp(min_y, max_y),
    }
}

pub fn desktop_pet_placement_for_window(
    position: DesktopPetPhysicalPosition,
    size: DesktopPetPhysicalSize,
    work_area: DesktopPetWorkArea,
) -> DesktopPetPlacementSnapshot {
    DesktopPetPlacementSnapshot {
        side: desktop_pet_side_for_position(position, size, work_area)
            .as_str()
            .to_string(),
    }
}

pub fn desktop_pet_side_for_position(
    position: DesktopPetPhysicalPosition,
    size: DesktopPetPhysicalSize,
    work_area: DesktopPetWorkArea,
) -> DesktopPetSide {
    let center_x = position.x + size.width / 2.0;
    let screen_mid_x = work_area.x + work_area.width / 2.0;
    if center_x > screen_mid_x {
        DesktopPetSide::Left
    } else {
        DesktopPetSide::Right
    }
}

pub fn desktop_pet_should_click_through(
    layout: DesktopPetHitLayout,
    cursor: DesktopPetPhysicalPosition,
    has_bubble: bool,
) -> bool {
    let scale_factor = normalized_scale_factor(layout.scale_factor);
    let local_x = (cursor.x - layout.position.x) / scale_factor;
    let local_y = (cursor.y - layout.position.y) / scale_factor;
    if local_x < 0.0 || local_y < 0.0 || local_x > layout.size.width || local_y > layout.size.height
    {
        return true;
    }
    !desktop_pet_local_point_is_hotspot(layout, local_x, local_y, has_bubble)
}

pub fn desktop_pet_local_point_is_hotspot(
    layout: DesktopPetHitLayout,
    x: f64,
    y: f64,
    has_bubble: bool,
) -> bool {
    let sprite_x = if layout.side == DesktopPetSide::Right {
        24.0 + DESKTOP_PET_SPRITE_VISIBLE_INSET_X
    } else {
        layout.size.width - 24.0 - DESKTOP_PET_SPRITE_SIZE + DESKTOP_PET_SPRITE_VISIBLE_INSET_X
    };
    let sprite_y =
        layout.size.height - 8.0 - DESKTOP_PET_SPRITE_SIZE + DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP;
    let sprite_width = DESKTOP_PET_SPRITE_SIZE - DESKTOP_PET_SPRITE_VISIBLE_INSET_X * 2.0;
    let sprite_height = DESKTOP_PET_SPRITE_SIZE
        - DESKTOP_PET_SPRITE_VISIBLE_INSET_TOP
        - DESKTOP_PET_SPRITE_VISIBLE_INSET_BOTTOM;
    let in_sprite = x >= sprite_x
        && x <= sprite_x + sprite_width
        && y >= sprite_y
        && y <= sprite_y + sprite_height;
    let in_bubble = if has_bubble {
        let bubble_x = if layout.side == DesktopPetSide::Right {
            layout.size.width - 8.0 - DESKTOP_PET_BUBBLE_WIDTH
        } else {
            8.0
        };
        let bubble_y = DESKTOP_PET_BUBBLE_TOP;
        x >= bubble_x
            && x <= bubble_x + DESKTOP_PET_BUBBLE_WIDTH
            && y >= bubble_y
            && y <= bubble_y + DESKTOP_PET_BUBBLE_HEIGHT
    } else {
        false
    };
    in_sprite || in_bubble
}

pub fn desktop_pet_raised_speech_frequency(value: &str) -> String {
    match value.trim() {
        "quiet" => "normal".to_string(),
        "normal" => "lively".to_string(),
        "lively" => "chatterbox".to_string(),
        "chatterbox" => "chatterbox".to_string(),
        _ => "lively".to_string(),
    }
}

pub fn desktop_pet_lowered_speech_frequency(value: &str) -> String {
    match value.trim() {
        "quiet" => "quiet".to_string(),
        "normal" => "quiet".to_string(),
        "lively" => "normal".to_string(),
        "chatterbox" => "lively".to_string(),
        _ => "quiet".to_string(),
    }
}

pub fn apply_desktop_pet_menu_action(
    support_dir: PathBuf,
    action_id: &str,
) -> Result<AppSettings, String> {
    let store = AppSettingsStore::from_support_dir(support_dir);
    store.update(|settings| match action_id {
        DESKTOP_PET_MUTE_30_MINUTES => {
            settings.ai.pet.speech_temporary_mute_until =
                Some(chrono::Utc::now().timestamp() + 1800);
        }
        DESKTOP_PET_MUTE_1_HOUR => {
            settings.ai.pet.speech_temporary_mute_until =
                Some(chrono::Utc::now().timestamp() + 3600);
        }
        DESKTOP_PET_MUTE_TODAY => {
            let tomorrow = chrono::Local::now()
                .date_naive()
                .succ_opt()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .and_then(|date| date.and_local_timezone(chrono::Local).single())
                .map(|date| date.timestamp())
                .unwrap_or_else(|| chrono::Utc::now().timestamp() + 86_400);
            settings.ai.pet.speech_temporary_mute_until = Some(tomorrow);
        }
        DESKTOP_PET_SPEAK_MORE => {
            settings.ai.pet.speech_frequency =
                desktop_pet_raised_speech_frequency(&settings.ai.pet.speech_frequency);
        }
        DESKTOP_PET_SPEAK_LESS => {
            settings.ai.pet.speech_frequency =
                desktop_pet_lowered_speech_frequency(&settings.ai.pet.speech_frequency);
        }
        DESKTOP_PET_HIDE => {
            settings.pet.desktop_widget = false;
        }
        DESKTOP_PET_SKIP_LINE => {}
        _ => {}
    })
}

fn valid_saved_origin(origin: DesktopPetSavedOrigin) -> Option<DesktopPetSavedOrigin> {
    if origin.x.is_finite() && origin.y.is_finite() {
        Some(origin)
    } else {
        None
    }
}

fn normalized_scale_factor(value: f64) -> f64 {
    value.max(0.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn clamps_default_initial_position_to_work_area() {
        let work_area = DesktopPetWorkArea {
            x: 0.0,
            y: 0.0,
            width: 1440.0,
            height: 900.0,
            scale_factor: 1.0,
        };
        let origin = desktop_pet_initial_position(None, work_area);
        assert_eq!(
            origin.x,
            1440.0 - DESKTOP_PET_BASE_WIDTH - DESKTOP_PET_MARGIN
        );
        assert_eq!(
            origin.y,
            900.0 - DESKTOP_PET_BASE_HEIGHT - DESKTOP_PET_DEFAULT_BOTTOM_MARGIN
        );
    }

    #[test]
    fn side_matches_tauri_bubble_side_semantics() {
        let work_area = DesktopPetWorkArea {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 800.0,
            scale_factor: 1.0,
        };
        let size = DesktopPetPhysicalSize {
            width: 200.0,
            height: 120.0,
        };
        assert_eq!(
            desktop_pet_side_for_position(
                DesktopPetPhysicalPosition { x: 760.0, y: 0.0 },
                size,
                work_area,
            ),
            DesktopPetSide::Left
        );
        assert_eq!(
            desktop_pet_side_for_position(
                DesktopPetPhysicalPosition { x: 20.0, y: 0.0 },
                size,
                work_area,
            ),
            DesktopPetSide::Right
        );
    }

    #[test]
    fn hit_test_accepts_sprite_and_optional_bubble() {
        let layout = DesktopPetHitLayout {
            position: DesktopPetPhysicalPosition { x: 100.0, y: 200.0 },
            size: DesktopPetPhysicalSize {
                width: DESKTOP_PET_BASE_WIDTH,
                height: DESKTOP_PET_BASE_HEIGHT,
            },
            scale_factor: 1.0,
            side: DesktopPetSide::Left,
        };
        assert!(!desktop_pet_should_click_through(
            layout,
            DesktopPetPhysicalPosition { x: 350.0, y: 300.0 },
            false,
        ));
        assert!(desktop_pet_should_click_through(
            layout,
            DesktopPetPhysicalPosition { x: 120.0, y: 260.0 },
            false,
        ));
        assert!(!desktop_pet_should_click_through(
            layout,
            DesktopPetPhysicalPosition { x: 120.0, y: 260.0 },
            true,
        ));
    }

    #[test]
    fn persists_only_finite_saved_origin() {
        let support_dir = temp_support_dir("desktop-pet-origin");
        let service = DesktopPetService::new(support_dir.clone());
        service
            .save_origin(DesktopPetSavedOrigin { x: 12.0, y: 34.0 })
            .expect("save origin");
        assert_eq!(
            service.saved_origin(),
            Some(DesktopPetSavedOrigin { x: 12.0, y: 34.0 })
        );
        assert!(
            service
                .save_origin(DesktopPetSavedOrigin {
                    x: f64::NAN,
                    y: 1.0
                })
                .is_err()
        );
        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn bubble_visibility_runtime_state_matches_tauri_hit_state() {
        let service = DesktopPetService::new(std::env::temp_dir());

        let hidden = service.set_bubble_visible(false);
        assert!(!hidden.bubble_visible);
        assert!(!service.bubble_visible());

        let visible = service.set_bubble_visible(true);
        assert!(visible.bubble_visible);
        assert!(service.bubble_visible());

        service.set_bubble_visible(false);
    }

    #[test]
    fn menu_action_updates_desktop_pet_settings() {
        let support_dir = temp_support_dir("desktop-pet-menu");
        let service = DesktopPetService::new(support_dir.clone());
        let settings = service
            .apply_menu_action(DESKTOP_PET_SPEAK_MORE)
            .expect("speak more");
        assert_eq!(settings.ai.pet.speech_frequency, "lively");
        let settings = service
            .apply_menu_action(DESKTOP_PET_HIDE)
            .expect("hide pet");
        assert!(!settings.pet.desktop_widget);
        fs::remove_dir_all(support_dir).ok();
    }

    fn temp_support_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("codux-{label}-{stamp}"))
    }
}
