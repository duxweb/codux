use super::*;

#[derive(Clone)]
struct DesktopPetLabels {
    mute_30: String,
    mute_1_hour: String,
    mute_today: String,
    skip_line: String,
    speak_more: String,
    speak_less: String,
    hide: String,
}

fn desktop_pet_labels(language: &str) -> DesktopPetLabels {
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    DesktopPetLabels {
        mute_30: tr("pet.desktop.mute_30_minutes", "Mute 30 Minutes"),
        mute_1_hour: tr("pet.desktop.mute_1_hour", "Mute 1 Hour"),
        mute_today: tr("pet.desktop.mute_today", "Mute Today"),
        skip_line: tr("pet.desktop.skip_line", "Skip Line"),
        speak_more: tr("pet.desktop.speak_more", "Speak More"),
        speak_less: tr("pet.desktop.speak_less", "Speak Less"),
        hide: tr("pet.desktop.hide", "Hide Desktop Pet"),
    }
}

pub(in crate::app) fn desktop_pet_menu_entries(
    language: &str,
) -> Vec<macos_window::NativeMenuEntry> {
    use macos_window::NativeMenuEntry::{Item, Separator};
    let labels = desktop_pet_labels(language);
    vec![
        Item {
            label: labels.mute_30,
            action_id: DESKTOP_PET_MUTE_30_MINUTES,
        },
        Item {
            label: labels.mute_1_hour,
            action_id: DESKTOP_PET_MUTE_1_HOUR,
        },
        Item {
            label: labels.mute_today,
            action_id: DESKTOP_PET_MUTE_TODAY,
        },
        Separator,
        Item {
            label: labels.skip_line,
            action_id: DESKTOP_PET_SKIP_LINE,
        },
        Item {
            label: labels.speak_more,
            action_id: DESKTOP_PET_SPEAK_MORE,
        },
        Item {
            label: labels.speak_less,
            action_id: DESKTOP_PET_SPEAK_LESS,
        },
        Separator,
        Item {
            label: labels.hide,
            action_id: DESKTOP_PET_HIDE,
        },
    ]
}

pub(in crate::app) fn desktop_pet_fallback_line() -> &'static str {
    ""
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum DesktopPetActivityTone {
    Normal,
    Attention,
    Success,
    Warning,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::app) struct DesktopPetAnimation {
    pub(in crate::app) row: usize,
    pub(in crate::app) frame_count: usize,
}

pub(in crate::app) struct DesktopPetActivityLine {
    pub(in crate::app) text: String,
    pub(in crate::app) tone: DesktopPetActivityTone,
}

impl DesktopPetActivityLine {
    fn empty() -> Self {
        Self {
            text: String::new(),
            tone: DesktopPetActivityTone::Normal,
        }
    }
}

pub(in crate::app) struct DesktopPetLlmContext {
    pub(in crate::app) event: &'static str,
    pub(in crate::app) fallback_text: String,
    pub(in crate::app) tone: DesktopPetActivityTone,
    pub(in crate::app) tool: String,
    pub(in crate::app) updated_at: f64,
}

fn replace_first_placeholder(template: String, value: &str) -> String {
    template.replacen("%@", value, 1)
}

fn replace_two_placeholders(template: String, first: &str, second: &str) -> String {
    template.replacen("%@", first, 1).replacen("%@", second, 1)
}

pub(in crate::app) fn desktop_pet_runtime_activity_line(
    runtime: &codux_runtime::ai_runtime_state::AIRuntimeStateSummary,
    language: &str,
) -> DesktopPetActivityLine {
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0);

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| {
            session.state == "needs-input"
                && session
                    .notification_type
                    .as_deref()
                    .map(is_permission_request_notification_type)
                    .unwrap_or(false)
        })
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
    {
        if let Some(target) = session
            .target_tool_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            return DesktopPetActivityLine {
                text: replace_two_placeholders(
                    tr(
                        "pet.activity.permission_waiting_target_format",
                        "%@ needs permission for %@",
                    ),
                    &session.tool,
                    target,
                ),
                tone: DesktopPetActivityTone::Attention,
            };
        }
        return DesktopPetActivityLine {
            text: replace_first_placeholder(
                tr(
                    "pet.activity.permission_waiting_format",
                    "%@ needs permission",
                ),
                &session.tool,
            ),
            tone: DesktopPetActivityTone::Attention,
        };
    }

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| session.state == "needs-input")
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
    {
        return DesktopPetActivityLine {
            text: normalized_desktop_pet_preview(session.message.as_deref()).unwrap_or_else(|| {
                replace_first_placeholder(
                    tr("pet.activity.waiting_input_format", "%@ needs input"),
                    &session.tool,
                )
            }),
            tone: DesktopPetActivityTone::Attention,
        };
    }

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| {
            session.state != "running"
                && session.state != "needs-input"
                && session.has_completed_turn
                && now - session.updated_at <= 30.0
        })
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
    {
        if session.was_interrupted {
            return DesktopPetActivityLine {
                text: replace_first_placeholder(
                    tr("pet.activity.failed_format", "%@ failed"),
                    &session.tool,
                ),
                tone: DesktopPetActivityTone::Warning,
            };
        }
        return DesktopPetActivityLine {
            text: replace_first_placeholder(
                tr("pet.activity.completed_format", "%@ completed"),
                &session.tool,
            ),
            tone: DesktopPetActivityTone::Success,
        };
    }

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| session.state == "running")
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
    {
        return DesktopPetActivityLine {
            text: normalized_desktop_pet_preview(session.latest_assistant_preview.as_deref())
                .unwrap_or_else(|| {
                    replace_first_placeholder(
                        tr("pet.activity.running_format", "%@ is running"),
                        &session.tool,
                    )
                }),
            tone: DesktopPetActivityTone::Normal,
        };
    }

    DesktopPetActivityLine::empty()
}

pub(in crate::app) fn desktop_pet_llm_context(
    runtime: &codux_runtime::ai_runtime_state::AIRuntimeStateSummary,
    language: &str,
) -> Option<DesktopPetLlmContext> {
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0);

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| {
            session.state == "needs-input"
                && session
                    .notification_type
                    .as_deref()
                    .map(is_permission_request_notification_type)
                    .unwrap_or(false)
        })
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
    {
        let fallback_text = if let Some(target) = session
            .target_tool_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            replace_two_placeholders(
                tr(
                    "pet.activity.permission_waiting_target_format",
                    "%@ needs permission for %@",
                ),
                &session.tool,
                target,
            )
        } else {
            replace_first_placeholder(
                tr(
                    "pet.activity.permission_waiting_format",
                    "%@ needs permission",
                ),
                &session.tool,
            )
        };
        return Some(DesktopPetLlmContext {
            event: "permission",
            fallback_text,
            tone: DesktopPetActivityTone::Attention,
            tool: session.tool.clone(),
            updated_at: session.updated_at,
        });
    }

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| session.state == "needs-input")
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
        .filter(|session| normalized_desktop_pet_preview(session.message.as_deref()).is_none())
    {
        return Some(DesktopPetLlmContext {
            event: "needsInput",
            fallback_text: replace_first_placeholder(
                tr("pet.activity.waiting_input_format", "%@ needs input"),
                &session.tool,
            ),
            tone: DesktopPetActivityTone::Attention,
            tool: session.tool.clone(),
            updated_at: session.updated_at,
        });
    }

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| {
            session.state != "running"
                && session.state != "needs-input"
                && session.has_completed_turn
                && now - session.updated_at <= 30.0
        })
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
    {
        let failed = session.was_interrupted;
        return Some(DesktopPetLlmContext {
            event: if failed { "failed" } else { "completed" },
            fallback_text: if failed {
                replace_first_placeholder(
                    tr("pet.activity.failed_format", "%@ failed"),
                    &session.tool,
                )
            } else {
                replace_first_placeholder(
                    tr("pet.activity.completed_format", "%@ completed"),
                    &session.tool,
                )
            },
            tone: if failed {
                DesktopPetActivityTone::Warning
            } else {
                DesktopPetActivityTone::Success
            },
            tool: session.tool.clone(),
            updated_at: session.updated_at,
        });
    }

    if let Some(session) = runtime
        .sessions
        .iter()
        .filter(|session| session.state == "running")
        .max_by(|left, right| left.updated_at.total_cmp(&right.updated_at))
        .filter(|session| {
            normalized_desktop_pet_preview(session.latest_assistant_preview.as_deref()).is_none()
        })
    {
        return Some(DesktopPetLlmContext {
            event: "running",
            fallback_text: replace_first_placeholder(
                tr("pet.activity.running_format", "%@ is running"),
                &session.tool,
            ),
            tone: DesktopPetActivityTone::Normal,
            tool: session.tool.clone(),
            updated_at: session.updated_at,
        });
    }

    None
}

pub(in crate::app) fn desktop_pet_llm_cooldown_seconds(value: &str) -> f64 {
    match value.trim() {
        "chatterbox" => 30.0,
        "lively" => 90.0,
        "quiet" => 15.0 * 60.0,
        _ => 5.0 * 60.0,
    }
}

pub(in crate::app) fn normalized_desktop_pet_preview(value: Option<&str>) -> Option<String> {
    let preview = value?
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    (!preview.is_empty()).then_some(preview)
}

fn is_permission_request_notification_type(value: &str) -> bool {
    matches!(
        value,
        "PermissionRequest" | "permission-request" | "permission_request"
    )
}

pub(in crate::app) const PET_ATLAS_COLUMNS: f32 = 8.0;
pub(in crate::app) const PET_ATLAS_ROWS: f32 = 9.0;
pub(in crate::app) const PET_ATLAS_CELL_WIDTH: f32 = 192.0;
pub(in crate::app) const PET_ATLAS_CELL_HEIGHT: f32 = 208.0;
pub(in crate::app) const PET_IDLE_FRAME_COUNT: usize = 6;
pub(in crate::app) const PET_RUNNING_FRAME_COUNT: usize = 6;
pub(in crate::app) const PET_WAITING_FRAME_COUNT: usize = 6;
pub(in crate::app) const PET_REVIEW_FRAME_COUNT: usize = 6;
pub(in crate::app) const PET_WAVING_FRAME_COUNT: usize = 4;
pub(in crate::app) const PET_FAILED_FRAME_COUNT: usize = 8;
pub(in crate::app) const DESKTOP_PET_SPRITE_SIZE: f32 = 112.0;
pub(in crate::app) const DESKTOP_PET_BUBBLE_WIDTH: f32 = 198.0;
pub(in crate::app) const DESKTOP_PET_BUBBLE_MIN_HEIGHT: f32 = 52.0;
pub(in crate::app) const DESKTOP_PET_BUBBLE_TOP: f32 = 52.0;
pub(in crate::app) const DESKTOP_PET_BUBBLE_EDGE: f32 = 8.0;
pub(in crate::app) const DESKTOP_PET_BUBBLE_TAIL_SIZE: f32 = 9.0;
pub(in crate::app) const DESKTOP_PET_SPRITE_BOTTOM: f32 = 8.0;
pub(in crate::app) const DESKTOP_PET_SPRITE_SIDE: f32 = 24.0;
pub(in crate::app) const DESKTOP_PET_FRAME_INTERVAL: Duration = Duration::from_secs(2);
pub(in crate::app) const DESKTOP_PET_ANIMATION_REST: Duration = Duration::from_secs(5);

pub(in crate::app) fn pet_sprite_visible_width(size: f32) -> f32 {
    PET_ATLAS_CELL_WIDTH * (size / PET_ATLAS_CELL_HEIGHT)
}

pub(in crate::app) fn pet_sprite_path(
    runtime_asset_root: &Path,
    support_dir: &Path,
    pet: &PetSummary,
    custom_pets: &[PetCustomPet],
) -> ImageSource {
    let fallback = "pets/voidcat/spritesheet.png".to_string();
    if let Some(custom_id) = pet.species.strip_prefix("custom:") {
        if let Some(custom_pet) = custom_pets.iter().find(|item| item.id == custom_id) {
            let path = support_dir
                .join("custom-pets")
                .join(&custom_pet.directory_name)
                .join(&custom_pet.spritesheet_path);
            if path.is_file() {
                return path.into();
            }
        }
        return fallback.into();
    }

    let species = pet.species.trim();
    let filesystem_path = runtime_asset_root
        .join("pets")
        .join(if species.is_empty() {
            "voidcat"
        } else {
            species
        })
        .join("spritesheet.png");
    if filesystem_path.is_file() {
        filesystem_path.into()
    } else {
        let species = if species.is_empty() {
            "voidcat"
        } else {
            species
        };
        format!("pets/{species}/spritesheet.png").into()
    }
}

pub(in crate::app) fn custom_pet_sprite_path(
    support_dir: &Path,
    custom_pet: &PetCustomPet,
) -> PathBuf {
    support_dir
        .join("custom-pets")
        .join(&custom_pet.directory_name)
        .join(&custom_pet.spritesheet_path)
}

pub(in crate::app) fn pet_sprite_path_cache(
    runtime_asset_root: &Path,
    support_dir: &Path,
    catalog: &PetCatalog,
) -> HashMap<String, ImageSource> {
    let mut paths = HashMap::new();
    for item in &catalog.species {
        paths.insert(
            item.species.clone(),
            pet_sprite_path(
                runtime_asset_root,
                support_dir,
                &PetSummary {
                    species: item.species.clone(),
                    ..PetSummary::default()
                },
                &[],
            ),
        );
    }
    for custom_pet in &catalog.custom_pets {
        paths.insert(
            format!("custom:{}", custom_pet.id),
            custom_pet_sprite_path(support_dir, custom_pet).into(),
        );
    }
    paths
}

pub(in crate::app) fn desktop_pet_sprite(
    sprite_path: ImageSource,
    frame: usize,
    row: usize,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    pet_sprite_element(
        sprite_path,
        DESKTOP_PET_SPRITE_SIZE,
        frame,
        row,
        cx.theme().primary,
    )
}

pub(in crate::app) fn desktop_pet_bubble(
    line: String,
    tone: DesktopPetActivityTone,
    left_tail: bool,
) -> AnyElement {
    let (fill, stroke, text) = match tone {
        DesktopPetActivityTone::Normal => (0x292B36, 0xFFFFFF, 0xF0EDE1),
        DesktopPetActivityTone::Attention => (0x6B330D, 0xFFAE38, 0xFFF1D6),
        DesktopPetActivityTone::Success => (0x144D29, 0x8CF275, 0xE1FFD1),
        DesktopPetActivityTone::Warning => (0x610D12, 0xFF6B5C, 0xFFE8E1),
    };
    let text_pad_left = if left_tail { 21.0 } else { 13.0 };
    let text_pad_right = if left_tail { 13.0 } else { 21.0 };

    div()
        .absolute()
        .top(px(DESKTOP_PET_BUBBLE_TOP))
        .w(px(DESKTOP_PET_BUBBLE_WIDTH))
        .min_h(px(DESKTOP_PET_BUBBLE_MIN_HEIGHT))
        .when(left_tail, |this| this.right(px(DESKTOP_PET_BUBBLE_EDGE)))
        .when(!left_tail, |this| this.left(px(DESKTOP_PET_BUBBLE_EDGE)))
        .child(pixel_bubble_canvas(stroke, fill, left_tail))
        .child(
            div()
                .relative()
                .min_h(px(DESKTOP_PET_BUBBLE_MIN_HEIGHT))
                .pt(px(10.0))
                .pb(px(10.0))
                .pl(px(text_pad_left))
                .pr(px(text_pad_right))
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .w(px(DESKTOP_PET_BUBBLE_WIDTH
                    - text_pad_left
                    - text_pad_right))
                .font_family("SF Mono")
                .text_size(rems(0.875))
                .line_height(rems(1.0625))
                .font_weight(FontWeight::BOLD)
                .text_center()
                .text_color(color(text))
                .line_clamp(3)
                .child(div().w_full().overflow_hidden().child(line)),
        )
        .into_any_element()
}

fn pixel_bubble_canvas(stroke_hex: u32, fill_hex: u32, left_tail: bool) -> AnyElement {
    let stroke = color(stroke_hex);
    let fill = color(fill_hex);
    canvas(
        move |_, _, _| {},
        move |bounds, _, window, _| {
            if let Ok(path) = pixel_bubble_path(bounds, 0.0, left_tail) {
                window.paint_path(path, stroke);
            }
            if let Ok(path) = pixel_bubble_path(bounds, 3.0, left_tail) {
                window.paint_path(path, fill);
            }
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

fn pixel_bubble_path(
    bounds: Bounds<Pixels>,
    inset: f32,
    left_tail: bool,
) -> Result<gpui::Path<Pixels>, anyhow::Error> {
    let width: f32 = bounds.size.width.into();
    let height: f32 = bounds.size.height.into();
    let tail = DESKTOP_PET_BUBBLE_TAIL_SIZE;
    let area_x = if left_tail { tail + inset } else { inset };
    let area_y = inset;
    let area_width = width - tail - inset * 2.0;
    let area_height = height - inset * 2.0;
    let x: f32 = bounds.origin.x.into();
    let y: f32 = bounds.origin.y.into();
    let mut builder = PathBuilder::fill();
    let points = pixel_bubble_points(area_width, area_height, left_tail);
    if let Some((first, rest)) = points.split_first() {
        builder.move_to(point(px(x + area_x + first.0), px(y + area_y + first.1)));
        for (px_x, px_y) in rest {
            builder.line_to(point(px(x + area_x + *px_x), px(y + area_y + *px_y)));
        }
        builder.line_to(point(px(x + area_x + first.0), px(y + area_y + first.1)));
    }
    builder.build()
}

fn pixel_bubble_points(width: f32, height: f32, left_tail: bool) -> Vec<(f32, f32)> {
    let step: f32 = 3.0;
    let tail = DESKTOP_PET_BUBBLE_TAIL_SIZE;
    let corner = step * 2.0;
    let tail_y = height / 2.0;

    if left_tail {
        vec![
            (0.0, tail_y - step),
            (step, tail_y - step),
            (step, tail_y - step * 2.0),
            (step * 2.0, tail_y - step * 2.0),
            (step * 2.0, tail_y - tail),
            (tail, tail_y - tail),
            (tail, corner),
            (tail + step, corner),
            (tail + step, step),
            (tail + corner, step),
            (tail + corner, 0.0),
            (width - corner, 0.0),
            (width - corner, step),
            (width - step, step),
            (width - step, corner),
            (width, corner),
            (width, height - corner),
            (width - step, height - corner),
            (width - step, height - step),
            (width - corner, height - step),
            (width - corner, height),
            (tail + corner, height),
            (tail + corner, height - step),
            (tail + step, height - step),
            (tail + step, height - corner),
            (tail, height - corner),
            (tail, tail_y + tail),
            (step * 2.0, tail_y + tail),
            (step * 2.0, tail_y + step * 2.0),
            (step, tail_y + step * 2.0),
            (step, tail_y + step),
            (0.0, tail_y + step),
        ]
    } else {
        vec![
            (0.0, corner),
            (step, corner),
            (step, step),
            (corner, step),
            (corner, 0.0),
            (width - step * 5.0, 0.0),
            (width - step * 5.0, step),
            (width - step * 4.0, step),
            (width - step * 4.0, corner),
            (width - tail, corner),
            (width - tail, tail_y - tail),
            (width - step * 2.0, tail_y - tail),
            (width - step * 2.0, tail_y - step * 2.0),
            (width - step, tail_y - step * 2.0),
            (width - step, tail_y - step),
            (width, tail_y - step),
            (width, tail_y + step),
            (width - step, tail_y + step),
            (width - step, tail_y + step * 2.0),
            (width - step * 2.0, tail_y + step * 2.0),
            (width - step * 2.0, tail_y + tail),
            (width - tail, tail_y + tail),
            (width - tail, height - corner),
            (width - step * 4.0, height - corner),
            (width - step * 4.0, height - step),
            (width - step * 5.0, height - step),
            (width - step * 5.0, height),
            (corner, height),
            (corner, height - step),
            (step, height - step),
            (step, height - corner),
            (0.0, height - corner),
        ]
    }
}

pub(in crate::app) fn pet_sprite_element(
    sprite_path: ImageSource,
    size: f32,
    frame: usize,
    row: usize,
    fallback_color: gpui::Hsla,
) -> AnyElement {
    let visible_width = pet_sprite_visible_width(size);
    let frame = frame % PET_ATLAS_COLUMNS as usize;
    let row = row % PET_ATLAS_ROWS as usize;
    let x_offset = -(frame as f32) * visible_width;
    let y_offset = -(row as f32) * size;

    div()
        .w(px(visible_width))
        .h(px(size))
        .overflow_hidden()
        .flex_none()
        .child(
            img(sprite_path)
                .w(px(PET_ATLAS_COLUMNS * visible_width))
                .h(px(PET_ATLAS_ROWS * size))
                .ml(px(x_offset))
                .mt(px(y_offset))
                .object_fit(ObjectFit::Fill)
                .with_fallback(move || {
                    div()
                        .size(px(size))
                        .rounded_full()
                        .bg(fallback_color.opacity(0.18))
                        .text_color(fallback_color)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(HeroIconName::Heart).size_6())
                        .into_any_element()
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_session(state: &str) -> codux_runtime::ai_runtime_state::AIRuntimeSessionSummary {
        codux_runtime::ai_runtime_state::AIRuntimeSessionSummary {
            terminal_id: "term-a".to_string(),
            project_id: "project-a".to_string(),
            project_path: None,
            tool: "codex".to_string(),
            ai_session_id: None,
            model: None,
            state: state.to_string(),
            project_name: "Codux".to_string(),
            session_title: "Session".to_string(),
            started_at: Some(1.0),
            updated_at: 2.0,
            event_count: 1,
            has_completed_turn: false,
            was_interrupted: false,
            notification_type: None,
            target_tool_name: None,
            message: None,
            latest_assistant_preview: None,
            total_tokens: 0,
            cached_input_tokens: 0,
            raw_total_tokens: 0,
            raw_cached_input_tokens: 0,
            baseline_total_tokens: 0,
            baseline_cached_input_tokens: 0,
            source: "runtime".to_string(),
        }
    }

    #[test]
    fn runtime_activity_line_uses_running_assistant_preview() {
        let mut session = runtime_session("running");
        session.latest_assistant_preview = Some("Analyzing files\n\nPreparing patch".to_string());
        let runtime = codux_runtime::ai_runtime_state::AIRuntimeStateSummary {
            sessions: vec![session],
            ..Default::default()
        };

        let line = desktop_pet_runtime_activity_line(&runtime, "zh-CN");

        assert_eq!(line.text, "Analyzing files\nPreparing patch");
        assert_eq!(line.tone, DesktopPetActivityTone::Normal);
    }

    #[test]
    fn runtime_activity_line_prioritizes_permission_requests() {
        let mut running = runtime_session("running");
        running.updated_at = 20.0;
        running.latest_assistant_preview = Some("Working".to_string());
        let mut permission = runtime_session("needs-input");
        permission.updated_at = 10.0;
        permission.notification_type = Some("PermissionRequest".to_string());
        permission.target_tool_name = Some("Write".to_string());
        let runtime = codux_runtime::ai_runtime_state::AIRuntimeStateSummary {
            sessions: vec![running, permission],
            ..Default::default()
        };

        let line = desktop_pet_runtime_activity_line(&runtime, "english");

        assert!(line.text.contains("codex"));
        assert!(line.text.contains("Write"));
        assert_eq!(line.tone, DesktopPetActivityTone::Attention);
    }

    #[test]
    fn runtime_activity_line_uses_needs_input_message() {
        let mut session = runtime_session("needs-input");
        session.message = Some("Choose an option\n\nthen continue".to_string());
        let runtime = codux_runtime::ai_runtime_state::AIRuntimeStateSummary {
            sessions: vec![session],
            ..Default::default()
        };

        let line = desktop_pet_runtime_activity_line(&runtime, "english");

        assert_eq!(line.text, "Choose an option\nthen continue");
        assert_eq!(line.tone, DesktopPetActivityTone::Attention);
    }

    #[test]
    fn runtime_activity_line_reports_recent_completion() {
        let mut session = runtime_session("completed");
        session.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        session.has_completed_turn = true;
        let runtime = codux_runtime::ai_runtime_state::AIRuntimeStateSummary {
            sessions: vec![session],
            ..Default::default()
        };

        let line = desktop_pet_runtime_activity_line(&runtime, "english");

        assert!(line.text.contains("codex"));
        assert_eq!(line.tone, DesktopPetActivityTone::Success);
    }

    #[test]
    fn runtime_activity_line_reports_recent_failure() {
        let mut session = runtime_session("completed");
        session.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        session.has_completed_turn = true;
        session.was_interrupted = true;
        let runtime = codux_runtime::ai_runtime_state::AIRuntimeStateSummary {
            sessions: vec![session],
            ..Default::default()
        };

        let line = desktop_pet_runtime_activity_line(&runtime, "english");

        assert!(line.text.contains("codex"));
        assert_eq!(line.tone, DesktopPetActivityTone::Warning);
    }
}
