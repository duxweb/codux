use gpui::{App, Hsla, SharedString, TitlebarOptions, Window, WindowAppearance, point, px, rgb};
use gpui_component::{Colorize, Theme, ThemeMode};
use std::sync::atomic::{AtomicU32, Ordering};

pub const BG: u32 = 0x0F1117;
pub const BG_ELEVATED: u32 = 0x161A23;
pub const BG_PANEL: u32 = 0x1B202B;
pub const BG_TERMINAL: u32 = 0x11141A;
pub const BG_COLUMN: u32 = 0x131821;
pub const BG_HEADER: u32 = 0x181D27;
pub const BG_ROW_HOVER: u32 = 0x202736;
pub const BG_ROW_ACTIVE: u32 = 0x19314E;
pub const BORDER: u32 = 0x2A3040;
pub const BORDER_SOFT: u32 = 0x2D3545;
pub const TEXT: u32 = 0xE7EAF0;
pub const TEXT_MUTED: u32 = 0x97A1B3;
pub const TEXT_DIM: u32 = 0x687286;
// Prefer the system accent once GPUI exposes it directly; blue is the fallback.
pub const ACCENT: u32 = 0x2F80ED;
pub const ORANGE: u32 = 0xE6A35C;
pub const GREEN: u32 = 0x78D891;
pub const RED: u32 = 0xF87171;
pub const STATUS_BAR: u32 = 0x1C1F25;

static DYNAMIC_BG: AtomicU32 = AtomicU32::new(BG);
static DYNAMIC_BG_ELEVATED: AtomicU32 = AtomicU32::new(BG_ELEVATED);
static DYNAMIC_BG_PANEL: AtomicU32 = AtomicU32::new(BG_PANEL);
static DYNAMIC_BG_TERMINAL: AtomicU32 = AtomicU32::new(BG_TERMINAL);
static DYNAMIC_BG_COLUMN: AtomicU32 = AtomicU32::new(BG_COLUMN);
static DYNAMIC_BG_HEADER: AtomicU32 = AtomicU32::new(BG_HEADER);
static DYNAMIC_BG_ROW_HOVER: AtomicU32 = AtomicU32::new(BG_ROW_HOVER);
static DYNAMIC_BG_ROW_ACTIVE: AtomicU32 = AtomicU32::new(BG_ROW_ACTIVE);
static DYNAMIC_BORDER: AtomicU32 = AtomicU32::new(BORDER);
static DYNAMIC_BORDER_SOFT: AtomicU32 = AtomicU32::new(BORDER_SOFT);
static DYNAMIC_TEXT: AtomicU32 = AtomicU32::new(TEXT);
static DYNAMIC_TEXT_MUTED: AtomicU32 = AtomicU32::new(TEXT_MUTED);
static DYNAMIC_TEXT_DIM: AtomicU32 = AtomicU32::new(TEXT_DIM);
static DYNAMIC_ACCENT: AtomicU32 = AtomicU32::new(ACCENT);
static DYNAMIC_ORANGE: AtomicU32 = AtomicU32::new(ORANGE);
static DYNAMIC_GREEN: AtomicU32 = AtomicU32::new(GREEN);
static DYNAMIC_RED: AtomicU32 = AtomicU32::new(RED);
static DYNAMIC_STATUS_BAR: AtomicU32 = AtomicU32::new(STATUS_BAR);

pub fn color(hex: u32) -> Hsla {
    rgb(dynamic_color(hex)).into()
}

pub fn fixed_color(hex: u32) -> Hsla {
    raw_color(hex)
}

fn raw_color(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn dynamic_color(hex: u32) -> u32 {
    match hex {
        BG => DYNAMIC_BG.load(Ordering::Relaxed),
        BG_ELEVATED => DYNAMIC_BG_ELEVATED.load(Ordering::Relaxed),
        BG_PANEL => DYNAMIC_BG_PANEL.load(Ordering::Relaxed),
        BG_TERMINAL => DYNAMIC_BG_TERMINAL.load(Ordering::Relaxed),
        BG_COLUMN => DYNAMIC_BG_COLUMN.load(Ordering::Relaxed),
        BG_HEADER => DYNAMIC_BG_HEADER.load(Ordering::Relaxed),
        BG_ROW_HOVER => DYNAMIC_BG_ROW_HOVER.load(Ordering::Relaxed),
        BG_ROW_ACTIVE => DYNAMIC_BG_ROW_ACTIVE.load(Ordering::Relaxed),
        BORDER => DYNAMIC_BORDER.load(Ordering::Relaxed),
        BORDER_SOFT => DYNAMIC_BORDER_SOFT.load(Ordering::Relaxed),
        TEXT => DYNAMIC_TEXT.load(Ordering::Relaxed),
        TEXT_MUTED => DYNAMIC_TEXT_MUTED.load(Ordering::Relaxed),
        TEXT_DIM => DYNAMIC_TEXT_DIM.load(Ordering::Relaxed),
        ACCENT => DYNAMIC_ACCENT.load(Ordering::Relaxed),
        ORANGE => DYNAMIC_ORANGE.load(Ordering::Relaxed),
        GREEN => DYNAMIC_GREEN.load(Ordering::Relaxed),
        RED => DYNAMIC_RED.load(Ordering::Relaxed),
        STATUS_BAR => DYNAMIC_STATUS_BAR.load(Ordering::Relaxed),
        _ => hex,
    }
}

fn set_dynamic_color(cell: &AtomicU32, value: u32) {
    cell.store(value, Ordering::Relaxed);
}

fn rgba_to_u32(value: Hsla) -> u32 {
    let rgba = value.to_rgb();
    let channel = |component: f32| -> u32 { (component.clamp(0.0, 1.0) * 255.0).round() as u32 };
    (channel(rgba.r) << 16) | (channel(rgba.g) << 8) | channel(rgba.b)
}

fn mix_hex(foreground: u32, background: u32, background_ratio: f32) -> u32 {
    let background_ratio = background_ratio.clamp(0.0, 1.0);
    let foreground_ratio = 1.0 - background_ratio;
    let channel = |shift: u32| -> u32 {
        let foreground_channel = ((foreground >> shift) & 0xFF) as f32;
        let background_channel = ((background >> shift) & 0xFF) as f32;
        (foreground_channel * foreground_ratio + background_channel * background_ratio).round()
            as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn mix_towards(color: Hsla, target: Hsla, amount: f32) -> Hsla {
    raw_color(mix_hex(rgba_to_u32(color), rgba_to_u32(target), amount))
}

pub fn divider_for_surface(surface: Hsla) -> Hsla {
    let rgb = surface.to_rgb();
    let luminance = 0.2126 * rgb.r + 0.7152 * rgb.g + 0.0722 * rgb.b;
    if luminance > 0.5 {
        raw_color(0x000000).opacity(0.10)
    } else {
        raw_color(0xFFFFFF).opacity(0.12)
    }
}

/// Default frosted-glass opacity for the UI chrome (used when no setting is
/// stored yet). The terminal body derives its own, more-opaque value from this.
pub const DEFAULT_VIBRANCY_ALPHA: f32 = 0.80;
/// Content panels (task column, right sidebar) sit this much more opaque than
/// the chrome so their lists/cards read clearly.
pub const PANEL_ALPHA_BOOST: f32 = 0.10;
/// The terminal body is kept this much more opaque than the UI chrome so the
/// shell text stays legible while the surrounding chrome reads as frosted glass.
pub const TERMINAL_ALPHA_BOOST: f32 = 0.20;

// 0 = solid (opaque), 1 = transparent (frosted). Alpha stored as f32 bits.
static DYNAMIC_WINDOW_TRANSPARENT: AtomicU32 = AtomicU32::new(1);
static DYNAMIC_VIBRANCY_ALPHA: AtomicU32 = AtomicU32::new(0); // set in `init` below

fn load_alpha(cell: &AtomicU32, fallback: f32) -> f32 {
    let bits = cell.load(Ordering::Relaxed);
    if bits == 0 {
        fallback
    } else {
        f32::from_bits(bits).clamp(0.0, 1.0)
    }
}

/// Whether the window paints solid (opaque) instead of frosted/transparent.
pub fn window_is_solid() -> bool {
    DYNAMIC_WINDOW_TRANSPARENT.load(Ordering::Relaxed) == 0
}

pub fn vibrancy_alpha() -> f32 {
    load_alpha(&DYNAMIC_VIBRANCY_ALPHA, DEFAULT_VIBRANCY_ALPHA)
}

/// The terminal body opacity, derived from the single UI opacity setting plus a
/// fixed boost so the terminal is always a bit more solid than the chrome.
pub fn terminal_alpha() -> f32 {
    (vibrancy_alpha() + TERMINAL_ALPHA_BOOST).min(1.0)
}

/// Apply the persisted appearance to the dynamic state read at render time.
pub fn set_window_appearance(transparent: bool, vibrancy_alpha: f32) {
    DYNAMIC_WINDOW_TRANSPARENT.store(transparent as u32, Ordering::Relaxed);
    set_vibrancy_alpha(vibrancy_alpha);
}

pub fn set_vibrancy_alpha(alpha: f32) {
    DYNAMIC_VIBRANCY_ALPHA.store(alpha.clamp(0.02, 1.0).to_bits(), Ordering::Relaxed);
}

/// Parse a stored opacity percentage string (e.g. "45") into a 0..1 fraction,
/// clamped to a sane range so the UI never becomes fully invisible.
pub fn opacity_fraction(percent: &str, default_percent: i64) -> f32 {
    let pct = percent
        .trim()
        .parse::<f64>()
        .map(|value| value.round() as i64)
        .unwrap_or(default_percent)
        .clamp(20, 100);
    pct as f32 / 100.0
}

/// Tint for frosted-glass regions (sidebar + column headers): a translucent fill
/// over the window's native blur material (macOS `NSVisualEffectView` / Windows
/// acrylic). In solid mode it returns the opaque base so callers degrade cleanly.
pub fn vibrancy(base: Hsla) -> Hsla {
    if window_is_solid() {
        base
    } else {
        base.opacity(vibrancy_alpha())
    }
}

/// Tint for content panels (task column, right sidebar): a step more opaque than
/// the chrome so their content reads clearly while still showing the blur.
pub fn vibrancy_panel(base: Hsla) -> Hsla {
    if window_is_solid() {
        base
    } else {
        base.opacity((vibrancy_alpha() + PANEL_ALPHA_BOOST).min(1.0))
    }
}

/// Whether the resolved app surface is dark — decides whether elevated surfaces
/// brighten (dark UI) or deepen (light UI). Reads the live app background.
fn surface_is_dark() -> bool {
    let bg = DYNAMIC_BG.load(Ordering::Relaxed);
    let r = ((bg >> 16) & 0xFF) as f32 / 255.0;
    let g = ((bg >> 8) & 0xFF) as f32 / 255.0;
    let b = (bg & 0xFF) as f32 / 255.0;
    0.2126 * r + 0.7152 * g + 0.0722 * b < 0.5
}

/// A solid surface one elevation step above (dark UI) / below (light UI) the
/// given base. Uses a *blended* tone — not a translucent overlay — so stacked
/// surfaces read as real depth instead of washing the frost out. `amount` is the
/// blend strength toward white / black.
pub fn elevate(base: Hsla, amount: f32) -> Hsla {
    let target = if surface_is_dark() {
        raw_color(0xFFFFFF)
    } else {
        raw_color(0x000000)
    };
    mix_towards(base, target, amount)
}

/// Raised chrome on a frosted panel — title / section bars AND cards. Blends the
/// base a step lighter (dark) / darker (light) so it reads as a genuine
/// elevation over the panel behind it, then inherits the panel opacity so it
/// still frosts. Opaque in solid mode.
pub fn vibrancy_raised(base: Hsla) -> Hsla {
    let raised = elevate(base, if surface_is_dark() { 0.07 } else { 0.06 });
    if window_is_solid() {
        raised
    } else {
        // Cards keep a high opacity floor so they stay legible even when the
        // user dials the window frost far down.
        raised.opacity((vibrancy_alpha() + PANEL_ALPHA_BOOST).clamp(0.85, 1.0))
    }
}

/// Tint for the terminal/workspace body backing. Translucent in transparent mode
/// so the blur shows behind the terminal; opaque in solid mode.
pub fn terminal_fill(base: Hsla) -> Hsla {
    if window_is_solid() {
        base
    } else {
        base.opacity(terminal_alpha())
    }
}

pub fn codux_main_titlebar(title: impl Into<SharedString>) -> TitlebarOptions {
    codux_titlebar(title, CoduxTitlebarKind::Main)
}

pub fn codux_child_titlebar(title: impl Into<SharedString>) -> TitlebarOptions {
    codux_titlebar(title, CoduxTitlebarKind::Child)
}

fn codux_titlebar(title: impl Into<SharedString>, kind: CoduxTitlebarKind) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title.into()),
        appears_transparent: true,
        traffic_light_position: codux_traffic_light_position(kind),
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum CoduxTitlebarKind {
    Main,
    Child,
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy)]
enum CoduxTitlebarKind {
    Main,
    Child,
}

#[cfg(target_os = "macos")]
fn codux_traffic_light_position(kind: CoduxTitlebarKind) -> Option<gpui::Point<gpui::Pixels>> {
    match kind {
        // Vertically center the lights in the 52px main header.
        CoduxTitlebarKind::Main => Some(point(px(12.0), px(20.0))),
        CoduxTitlebarKind::Child => Some(point(px(12.0), px(17.0))),
    }
}

#[cfg(not(target_os = "macos"))]
fn codux_traffic_light_position(_kind: CoduxTitlebarKind) -> Option<gpui::Point<gpui::Pixels>> {
    None
}

pub fn apply_component_theme(
    theme_name: &str,
    theme_color: &str,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    let appearance = window
        .as_ref()
        .map(|window| window.appearance())
        .unwrap_or_else(|| cx.window_appearance());
    apply_component_theme_for_appearance(theme_name, theme_color, appearance, window, cx);
}

pub fn apply_component_theme_for_appearance(
    theme_name: &str,
    theme_color: &str,
    appearance: WindowAppearance,
    mut window: Option<&mut Window>,
    cx: &mut App,
) {
    let terminal = terminal_theme_palette_for_appearance(theme_name, appearance);
    let app = app_theme_palette_for_appearance(theme_name, appearance);
    let mode = if terminal.is_light {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    Theme::change(mode, window.as_deref_mut(), cx);

    configure_component_theme(cx, terminal, app, theme_color_value(theme_color));
    cx.refresh_windows();
    if let Some(window) = window {
        window.refresh();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalThemePalette {
    pub background: u32,
    pub foreground: u32,
    pub cursor: u32,
    pub selection: u32,
    pub black: u32,
    pub red: u32,
    pub green: u32,
    pub yellow: u32,
    pub blue: u32,
    pub magenta: u32,
    pub cyan: u32,
    pub white: u32,
    pub bright_black: u32,
    pub bright_red: u32,
    pub bright_green: u32,
    pub bright_yellow: u32,
    pub bright_blue: u32,
    pub bright_magenta: u32,
    pub bright_cyan: u32,
    pub bright_white: u32,
    pub muted_foreground: u32,
    pub is_light: bool,
}

/// Designed app-chrome tokens for a Codux collection theme. Every surface,
/// line and text tone is hand-picked instead of derived from the terminal
/// palette, so panels/borders/hierarchy read as intentional design.
#[derive(Clone, Copy, Debug)]
pub struct AppThemePalette {
    pub ground: u32,
    pub column: u32,
    pub panel: u32,
    pub header: u32,
    pub card: u32,
    pub elevated: u32,
    pub border: u32,
    pub border_soft: u32,
    pub muted: u32,
    pub text: u32,
    pub text_muted: u32,
    pub text_dim: u32,
    pub status_bar: u32,
    pub row_hover: u32,
}

impl TerminalThemePalette {
    fn auto(is_light: bool) -> Self {
        if is_light {
            return terminal_theme_palette("Codux Light");
        }
        terminal_theme_palette("Codux Dark")
    }

    fn from_colors(
        is_light: bool,
        background: u32,
        foreground: u32,
        cursor: u32,
        selection: u32,
        ansi: [u32; 8],
        bright_ansi: [u32; 8],
    ) -> Self {
        let [black, red, green, yellow, blue, magenta, cyan, white] = ansi;
        let [
            bright_black,
            bright_red,
            bright_green,
            bright_yellow,
            bright_blue,
            bright_magenta,
            bright_cyan,
            bright_white,
        ] = bright_ansi;
        Self {
            background,
            foreground,
            cursor,
            selection,
            black,
            red,
            green,
            yellow,
            blue,
            magenta,
            cyan,
            white,
            bright_black,
            bright_red,
            bright_green,
            bright_yellow,
            bright_blue,
            bright_magenta,
            bright_cyan,
            bright_white,
            muted_foreground: mix_hex(foreground, background, if is_light { 0.42 } else { 0.36 }),
            is_light,
        }
    }
}

pub fn terminal_theme_palette_for_appearance(
    theme_name: &str,
    appearance: WindowAppearance,
) -> TerminalThemePalette {
    if normalize_theme_name(theme_name) == "auto" {
        return TerminalThemePalette::auto(matches!(
            appearance,
            WindowAppearance::Light | WindowAppearance::VibrantLight
        ));
    }
    terminal_theme_palette(theme_name)
}

pub fn terminal_theme_palette(theme_name: &str) -> TerminalThemePalette {
    match normalize_theme_name(theme_name).as_str() {
        "auto" => TerminalThemePalette::auto(false),
        "codux dark" => TerminalThemePalette::from_colors(
            false,
            0x141821,
            0xD8DCE6,
            0x7AB8FF,
            0x2C4A75,
            [
                0x1E232E, 0xF27878, 0x82D896, 0xE8C66A, 0x74AEF6, 0xC793E8, 0x6CD3E0, 0xD8DCE6,
            ],
            [
                0x59647A, 0xFF9E9E, 0xA9EDB8, 0xF4D98C, 0x9FC9FF, 0xDDB3F5, 0x9EEAF2, 0xF5F7FA,
            ],
        ),
        "deep ocean" => TerminalThemePalette::from_colors(
            false,
            0x0C1626,
            0xCCDCF2,
            0x53C0F0,
            0x1D3D66,
            [
                0x16233A, 0xF07886, 0x5FD4A2, 0xE8C170, 0x5AA7F0, 0xB48EF2, 0x54CBE0, 0xCCDCF2,
            ],
            [
                0x4A6284, 0xFF9DA8, 0x8CEBC2, 0xF5D694, 0x8CC5FF, 0xCFB0FF, 0x86E5F2, 0xF0F6FF,
            ],
        ),
        "arctic night" => TerminalThemePalette::from_colors(
            false,
            0x262B36,
            0xD8DEE9,
            0x88C0D0,
            0x434C5E,
            [
                0x3B4252, 0xBF616A, 0xA3BE8C, 0xEBCB8B, 0x81A1C1, 0xB48EAD, 0x88C0D0, 0xE5E9F0,
            ],
            [
                0x4C566A, 0xD87B84, 0xB5D19A, 0xF2DCA2, 0x94B4D4, 0xC79EC4, 0x9BD3E0, 0xECEFF4,
            ],
        ),
        "forest night" => TerminalThemePalette::from_colors(
            false,
            0x101915,
            0xD3E2D6,
            0x7FD8A0,
            0x27503A,
            [
                0x1B2921, 0xEF7B7B, 0x7CD98F, 0xD9C36A, 0x6FB3E8, 0xC08FE0, 0x63D6C2, 0xD3E2D6,
            ],
            [
                0x50685A, 0xFFA2A2, 0xA5EDB4, 0xEDD98F, 0x9BCDF5, 0xD8B2F0, 0x92EBDC, 0xF0F7F1,
            ],
        ),
        "ember" => TerminalThemePalette::from_colors(
            false,
            0x1A1512,
            0xE8DDD0,
            0xF0A860,
            0x54402C,
            [
                0x2A221B, 0xF2766B, 0x9CCE6E, 0xF0B860, 0x6FAEE0, 0xD892C0, 0x72CCB8, 0xE8DDD0,
            ],
            [
                0x6E5D4C, 0xFF9C8F, 0xBEE393, 0xFFD088, 0x9CC9F0, 0xEDB4D8, 0x9CE3D2, 0xFAF4EC,
            ],
        ),
        "amethyst dusk" => TerminalThemePalette::from_colors(
            false,
            0x161326,
            0xDCD8EE,
            0xA88BF0,
            0x3A2F66,
            [
                0x232040, 0xF27A93, 0x7FD8A8, 0xE5C578, 0x7FA6F5, 0xB78CF2, 0x76CBE8, 0xDCD8EE,
            ],
            [
                0x5A5482, 0xFFA0B4, 0xA6EDC7, 0xF3DA9E, 0xA8C4FF, 0xD1AFFF, 0xA0E3F5, 0xF4F2FC,
            ],
        ),
        "rose noir" => TerminalThemePalette::from_colors(
            false,
            0x1C141B,
            0xEADDE5,
            0xEB8FB0,
            0x523248,
            [
                0x2E2130, 0xEB6F92, 0x86CFA4, 0xF6C177, 0x7A9BE8, 0xC4A7E7, 0x9CCFD8, 0xEADDE5,
            ],
            [
                0x6E5A6A, 0xFF94B2, 0xA9E5C4, 0xFFD69C, 0xA4BEFF, 0xDCC5F7, 0xBCE3EB, 0xFBF5F9,
            ],
        ),
        "carbon" => TerminalThemePalette::from_colors(
            false,
            0x131313,
            0xE6E6E6,
            0xFFFFFF,
            0x3A3A3A,
            [
                0x222222, 0xF07070, 0x7BD88F, 0xE6C568, 0x6EB2F0, 0xC490E4, 0x66D4DE, 0xE6E6E6,
            ],
            [
                0x666666, 0xFF9B9B, 0xA6EDB4, 0xF4DA90, 0x9CCCFF, 0xDCB4F5, 0x96EAF0, 0xFFFFFF,
            ],
        ),
        "codux light" => TerminalThemePalette::from_colors(
            true,
            0xFAFBFC,
            0x2A3140,
            0x2F6FE4,
            0xCCE0FA,
            [
                0x323A48, 0xC93A46, 0x1E9E50, 0xB07D10, 0x2264D0, 0x9440C4, 0x0E8CA6, 0x99A2B2,
            ],
            [
                0x5C6678, 0xE45560, 0x2CBA66, 0xCC9718, 0x3E82EA, 0xAC62DA, 0x1AA8C4, 0xB8C0CC,
            ],
        ),
        "glacier" => TerminalThemePalette::from_colors(
            true,
            0xF7FAFD,
            0x243648,
            0x0E8CD0,
            0xC4E0F5,
            [
                0x2C3E52, 0xC94257, 0x148A62, 0xA67C14, 0x1272C8, 0x8A4CC8, 0x0A8FB0, 0x94A6B8,
            ],
            [
                0x5A7086, 0xE0596E, 0x1FA878, 0xC2951C, 0x2E8AE0, 0xA468DE, 0x16A9CC, 0xB9C8D6,
            ],
        ),
        "morning mist" => TerminalThemePalette::from_colors(
            true,
            0xFAFAFB,
            0x33373E,
            0x5B6472,
            0xD5DAE2,
            [
                0x3A3F47, 0xC44540, 0x2E8B57, 0xA97B18, 0x3B6EC8, 0x8F52B8, 0x2090A0, 0x9CA1A9,
            ],
            [
                0x62676F, 0xDE5F5A, 0x3EA76D, 0xC49422, 0x5586DC, 0xA96ED0, 0x30AABC, 0xC0C4CB,
            ],
        ),
        "matcha" => TerminalThemePalette::from_colors(
            true,
            0xF8FBF6,
            0x2C3A2A,
            0x3E9450,
            0xCFE8CC,
            [
                0x33422F, 0xBE4A42, 0x2E8B3C, 0x9C801A, 0x2E6EB8, 0x8A54B0, 0x188E86, 0x97A692,
            ],
            [
                0x5E7059, 0xD8645C, 0x3FA850, 0xB59A24, 0x4886D0, 0xA670CC, 0x24A89E, 0xBCC9B7,
            ],
        ),
        "ivory" => TerminalThemePalette::from_colors(
            true,
            0xFCF9F2,
            0x3A3226,
            0xC28418,
            0xF0E0BC,
            [
                0x423A2C, 0xC24438, 0x4E8A28, 0xA87814, 0x3268B4, 0x9A4CA8, 0x1E8E7E, 0xA89B84,
            ],
            [
                0x6E624E, 0xDC5E52, 0x66A63A, 0xC4931E, 0x4C82CC, 0xB468C2, 0x2AA896, 0xCCC0A8,
            ],
        ),
        "lavender" => TerminalThemePalette::from_colors(
            true,
            0xFAF9FD,
            0x322B44,
            0x7C5CD6,
            0xDCD2F5,
            [
                0x3A3350, 0xC24468, 0x2E8B5E, 0xA6791C, 0x4A62D8, 0x8C48C8, 0x2288A8, 0x9E97B2,
            ],
            [
                0x645C7E, 0xDC6084, 0x3FA674, 0xC29426, 0x667EE8, 0xA666DC, 0x30A2C2, 0xC4BED4,
            ],
        ),
        "rosewater" => TerminalThemePalette::from_colors(
            true,
            0xFCF8F9,
            0x3C2B33,
            0xC2426E,
            0xF4D6DF,
            [
                0x443039, 0xC23A55, 0x2E8B57, 0xA87818, 0x3766C4, 0xA4479E, 0x1E8C96, 0xAA96A0,
            ],
            [
                0x705C66, 0xDC5674, 0x3FA76D, 0xC49322, 0x5380DC, 0xC062BA, 0x2AA6B2, 0xD0BEC6,
            ],
        ),
        "sandstone" => TerminalThemePalette::from_colors(
            true,
            0xFBF7EA,
            0x413F30,
            0x1E8E8E,
            0xEBE0BC,
            [
                0x45422F, 0xBE4632, 0x5D8A16, 0xA88410, 0x2E74B8, 0xA0489C, 0x1A9090, 0xA49E86,
            ],
            [
                0x6E6852, 0xD8604C, 0x76A626, 0xC49E1A, 0x488CCE, 0xBA64B4, 0x28AAAA, 0xC9C2A8,
            ],
        ),
        _ => TerminalThemePalette::auto(false),
    }
}

pub fn app_theme_palette_for_appearance(
    theme_name: &str,
    appearance: WindowAppearance,
) -> AppThemePalette {
    let name = normalize_theme_name(theme_name);
    let name = if name == "auto" {
        let is_light = matches!(
            appearance,
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        if is_light {
            "codux light"
        } else {
            "codux dark"
        }
        .to_string()
    } else {
        name
    };
    // Unknown / legacy persisted names fall back with the terminal palette.
    app_theme_palette(&name)
        .or_else(|| app_theme_palette("codux dark"))
        .expect("flagship app palette")
}

/// Designed chrome palettes for the Codux theme collection.
pub fn app_theme_palette(normalized_name: &str) -> Option<AppThemePalette> {
    let palette = match normalized_name {
        "codux dark" => AppThemePalette {
            ground: 0x12151C,
            column: 0x151922,
            panel: 0x191E28,
            header: 0x1B2029,
            card: 0x2A3140,
            elevated: 0x303848,
            border: 0x323A4A,
            border_soft: 0x2A3140,
            muted: 0x1F2530,
            text: 0xE8EBF2,
            text_muted: 0xA3ACBE,
            text_dim: 0x707A8E,
            status_bar: 0x161A22,
            row_hover: 0x232A38,
        },
        "deep ocean" => AppThemePalette {
            ground: 0x0A1220,
            column: 0x0C1526,
            panel: 0x0F1A2E,
            header: 0x101C32,
            card: 0x1D3050,
            elevated: 0x24395E,
            border: 0x264066,
            border_soft: 0x1E3352,
            muted: 0x142138,
            text: 0xD9E4F5,
            text_muted: 0x8FA5C6,
            text_dim: 0x5F7699,
            status_bar: 0x0C1523,
            row_hover: 0x182A45,
        },
        "arctic night" => AppThemePalette {
            ground: 0x232833,
            column: 0x262B37,
            panel: 0x2B313E,
            header: 0x2D3341,
            card: 0x3D4557,
            elevated: 0x434C60,
            border: 0x465064,
            border_soft: 0x3C4556,
            muted: 0x313847,
            text: 0xE5E9F0,
            text_muted: 0xAAB4C5,
            text_dim: 0x7B879B,
            status_bar: 0x262B36,
            row_hover: 0x333A4A,
        },
        "forest night" => AppThemePalette {
            ground: 0x0E1512,
            column: 0x111915,
            panel: 0x152019,
            header: 0x17231C,
            card: 0x24382C,
            elevated: 0x2A4033,
            border: 0x2F4638,
            border_soft: 0x27392E,
            muted: 0x1A2820,
            text: 0xE2EBE4,
            text_muted: 0x9DB3A4,
            text_dim: 0x6B8172,
            status_bar: 0x101813,
            row_hover: 0x1E2E24,
        },
        "ember" => AppThemePalette {
            ground: 0x171310,
            column: 0x1B1613,
            panel: 0x211B16,
            header: 0x241D18,
            card: 0x352C23,
            elevated: 0x3C3228,
            border: 0x453930,
            border_soft: 0x392F27,
            muted: 0x282019,
            text: 0xF0E8E0,
            text_muted: 0xBCA997,
            text_dim: 0x8A7A6A,
            status_bar: 0x1A1512,
            row_hover: 0x2E251D,
        },
        "amethyst dusk" => AppThemePalette {
            ground: 0x141220,
            column: 0x171527,
            panel: 0x1C1930,
            header: 0x1F1B35,
            card: 0x2F2950,
            elevated: 0x363058,
            border: 0x3D3563,
            border_soft: 0x332C52,
            muted: 0x231F3B,
            text: 0xE9E6F5,
            text_muted: 0xAAA3C8,
            text_dim: 0x776F9B,
            status_bar: 0x161428,
            row_hover: 0x292345,
        },
        "rose noir" => AppThemePalette {
            ground: 0x1A1219,
            column: 0x1E151D,
            panel: 0x241A23,
            header: 0x271C26,
            card: 0x392B37,
            elevated: 0x413240,
            border: 0x4A3746,
            border_soft: 0x3D2E3A,
            muted: 0x2B1F29,
            text: 0xF2E7EE,
            text_muted: 0xC0A6B8,
            text_dim: 0x8D7386,
            status_bar: 0x1C141B,
            row_hover: 0x322430,
        },
        "carbon" => AppThemePalette {
            ground: 0x111111,
            column: 0x141414,
            panel: 0x191919,
            header: 0x1C1C1C,
            card: 0x2B2B2B,
            elevated: 0x323232,
            border: 0x393939,
            border_soft: 0x2E2E2E,
            muted: 0x202020,
            text: 0xF2F2F2,
            text_muted: 0xABABAB,
            text_dim: 0x777777,
            status_bar: 0x151515,
            row_hover: 0x252525,
        },
        "codux light" => AppThemePalette {
            ground: 0xEEF0F4,
            column: 0xF2F4F7,
            panel: 0xF8F9FB,
            header: 0xF4F6F9,
            card: 0xFFFFFF,
            elevated: 0xFFFFFF,
            border: 0xD5DAE3,
            border_soft: 0xE1E5EC,
            muted: 0xE7EAF0,
            text: 0x1D2430,
            text_muted: 0x4E5A6E,
            text_dim: 0x7A8496,
            status_bar: 0xF2F4F7,
            row_hover: 0xE6EAF1,
        },
        "glacier" => AppThemePalette {
            ground: 0xE9F0F6,
            column: 0xEDF3F8,
            panel: 0xF5F9FC,
            header: 0xF0F5FA,
            card: 0xFFFFFF,
            elevated: 0xFFFFFF,
            border: 0xCBD9E6,
            border_soft: 0xDAE5EE,
            muted: 0xE2EBF2,
            text: 0x16283A,
            text_muted: 0x47607A,
            text_dim: 0x7288A0,
            status_bar: 0xEDF3F8,
            row_hover: 0xDFEAF3,
        },
        "morning mist" => AppThemePalette {
            ground: 0xEDEEF1,
            column: 0xF1F2F4,
            panel: 0xF7F8F9,
            header: 0xF3F4F6,
            card: 0xFDFDFE,
            elevated: 0xFFFFFF,
            border: 0xD6D8DE,
            border_soft: 0xE2E4E9,
            muted: 0xE8E9ED,
            text: 0x26292F,
            text_muted: 0x555A64,
            text_dim: 0x82878F,
            status_bar: 0xF1F2F4,
            row_hover: 0xE5E7EB,
        },
        "matcha" => AppThemePalette {
            ground: 0xEAF0E8,
            column: 0xEEF3EC,
            panel: 0xF6F9F4,
            header: 0xF1F5EF,
            card: 0xFDFEFC,
            elevated: 0xFFFFFF,
            border: 0xCFDCCB,
            border_soft: 0xDDE6D9,
            muted: 0xE3EBE0,
            text: 0x1F2B1E,
            text_muted: 0x4D5F4A,
            text_dim: 0x788B74,
            status_bar: 0xEEF3EC,
            row_hover: 0xE0E9DC,
        },
        "ivory" => AppThemePalette {
            ground: 0xF2EDE3,
            column: 0xF5F0E7,
            panel: 0xFAF6EE,
            header: 0xF6F2E9,
            card: 0xFFFDF8,
            elevated: 0xFFFEFA,
            border: 0xDED4C2,
            border_soft: 0xE8E0D1,
            muted: 0xEDE6D8,
            text: 0x322A1E,
            text_muted: 0x655A47,
            text_dim: 0x91856F,
            status_bar: 0xF5F0E7,
            row_hover: 0xEAE2D2,
        },
        "lavender" => AppThemePalette {
            ground: 0xEEECF5,
            column: 0xF1F0F8,
            panel: 0xF8F7FC,
            header: 0xF3F2FA,
            card: 0xFEFEFF,
            elevated: 0xFFFFFF,
            border: 0xD8D3E8,
            border_soft: 0xE3DFF0,
            muted: 0xE9E6F3,
            text: 0x272138,
            text_muted: 0x585070,
            text_dim: 0x847C9C,
            status_bar: 0xF1F0F8,
            row_hover: 0xE4E0F0,
        },
        "rosewater" => AppThemePalette {
            ground: 0xF4ECEE,
            column: 0xF6F0F1,
            panel: 0xFBF6F7,
            header: 0xF7F1F3,
            card: 0xFFFDFD,
            elevated: 0xFFFFFF,
            border: 0xE2D2D7,
            border_soft: 0xEBDFE2,
            muted: 0xEFE4E7,
            text: 0x33222A,
            text_muted: 0x685260,
            text_dim: 0x957E8A,
            status_bar: 0xF6F0F1,
            row_hover: 0xECDFE3,
        },
        "sandstone" => AppThemePalette {
            ground: 0xEFE9DA,
            column: 0xF2ECDE,
            panel: 0xF8F3E7,
            header: 0xF4EEE1,
            card: 0xFDFAF1,
            elevated: 0xFFFCF4,
            border: 0xDACFB4,
            border_soft: 0xE5DCC6,
            muted: 0xEAE2CE,
            text: 0x322E24,
            text_muted: 0x635E4E,
            text_dim: 0x8F8873,
            status_bar: 0xF2ECDE,
            row_hover: 0xE7DEC8,
        },
        _ => return None,
    };
    Some(palette)
}

fn normalize_theme_name(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn theme_color_value(theme_color: &str) -> u32 {
    match theme_color.to_ascii_lowercase().as_str() {
        "sky" => 0x0EA5E9,
        "cyan" => 0x06B6D4,
        "teal" => 0x14B8A6,
        "emerald" | "moss" => 0x10B981,
        "green" | "sage" => 0x22C55E,
        "lime" => 0x84CC16,
        "amber" | "gold" => 0xF59E0B,
        "orange" | "burnt" => 0xF97316,
        "red" | "crimson" => 0xEF4444,
        "rose" | "plum" => 0xF43F5E,
        "pink" => 0xEC4899,
        "fuchsia" => 0xD946EF,
        "purple" => 0xA855F7,
        "violet" | "iris" | "lavender" => 0x8B5CF6,
        "indigo" => 0x6366F1,
        _ => 0x3B82F6,
    }
}

fn configure_component_theme(
    cx: &mut App,
    terminal: TerminalThemePalette,
    app: AppThemePalette,
    accent_hex: u32,
) {
    let is_dark = !terminal.is_light;
    let theme = Theme::global_mut(cx);
    let accent_color = raw_color(accent_hex);

    // Mode-scoped values, independent of the surface palette.
    let (control_bg, control_hover, input) = if is_dark {
        (
            raw_color(0xFFFFFF).opacity(0.09),
            raw_color(0xFFFFFF).opacity(0.14),
            raw_color(0xFFFFFF).opacity(0.075),
        )
    } else {
        (
            raw_color(0x000000).opacity(0.055),
            raw_color(0x000000).opacity(0.085),
            raw_color(0x000000).opacity(0.075),
        )
    };
    let accent = accent_color;
    let accent_bg = accent_color.opacity(if is_dark { 0.17 } else { 0.12 });
    let (primary_hover, primary_active, primary_foreground) = if is_dark {
        (
            mix_towards(accent_color, raw_color(0xFFFFFF), 0.18),
            mix_towards(accent_color, raw_color(0x000000), 0.16),
            raw_color(0xF6FAFF),
        )
    } else {
        (
            mix_towards(accent_color, raw_color(0x000000), 0.12),
            mix_towards(accent_color, raw_color(0x000000), 0.22),
            raw_color(0xFFFFFF),
        )
    };
    let overlay = raw_color(0x000000).opacity(if is_dark { 0.42 } else { 0.28 });
    let (danger, warning, success, info) = if is_dark {
        (
            raw_color(RED),
            raw_color(ORANGE),
            raw_color(GREEN),
            raw_color(0x60A5FA),
        )
    } else {
        (
            raw_color(0xDC2626),
            raw_color(0xD97706),
            raw_color(0x16A34A),
            raw_color(0x2563EB),
        )
    };
    let hover_surface = if is_dark {
        raw_color(0xFFFFFF).opacity(0.15)
    } else {
        raw_color(0x000000).opacity(0.07)
    };

    // Surfaces: every tone comes from the designed app palette.
    let background = raw_color(app.ground);
    let foreground = raw_color(app.text);
    let muted = raw_color(app.muted);
    let muted_foreground = raw_color(app.text_dim);
    let border = raw_color(app.border);
    let border_soft = raw_color(app.border_soft);
    let popover = raw_color(app.elevated);
    let sidebar = raw_color(app.panel);
    let header = raw_color(app.header);
    let row_hover = raw_color(app.row_hover);
    let title_bar = raw_color(app.status_bar);
    let tab = raw_color(app.ground);
    let tab_bar = raw_color(app.header);
    let scrollbar_thumb = mix_towards(border, muted_foreground, 0.45);
    let task_column = raw_color(app.column);
    let panel_surface = raw_color(app.panel);
    let card_surface = raw_color(app.card);
    let text = raw_color(app.text);
    let text_muted = raw_color(app.text_muted);

    set_dynamic_color(&DYNAMIC_BG, rgba_to_u32(background));
    set_dynamic_color(&DYNAMIC_BG_ELEVATED, rgba_to_u32(popover));
    set_dynamic_color(&DYNAMIC_BG_PANEL, rgba_to_u32(panel_surface));
    set_dynamic_color(&DYNAMIC_BG_TERMINAL, terminal.background);
    set_dynamic_color(&DYNAMIC_BG_COLUMN, rgba_to_u32(task_column));
    set_dynamic_color(&DYNAMIC_BG_HEADER, rgba_to_u32(header));
    set_dynamic_color(&DYNAMIC_BG_ROW_HOVER, rgba_to_u32(row_hover));
    set_dynamic_color(&DYNAMIC_BG_ROW_ACTIVE, rgba_to_u32(accent_bg));
    set_dynamic_color(&DYNAMIC_BORDER, rgba_to_u32(border));
    set_dynamic_color(&DYNAMIC_BORDER_SOFT, rgba_to_u32(border_soft));
    set_dynamic_color(&DYNAMIC_TEXT, rgba_to_u32(text));
    set_dynamic_color(&DYNAMIC_TEXT_MUTED, rgba_to_u32(text_muted));
    set_dynamic_color(&DYNAMIC_TEXT_DIM, rgba_to_u32(muted_foreground));
    set_dynamic_color(&DYNAMIC_ACCENT, accent_hex);
    // Keep the semantic triad mode-aware so `color(GREEN/ORANGE/RED)` callers
    // (incl. cx-less helpers) follow light/dark automatically.
    set_dynamic_color(&DYNAMIC_ORANGE, rgba_to_u32(warning));
    set_dynamic_color(&DYNAMIC_GREEN, rgba_to_u32(success));
    set_dynamic_color(&DYNAMIC_RED, rgba_to_u32(danger));
    set_dynamic_color(&DYNAMIC_STATUS_BAR, rgba_to_u32(title_bar));

    theme.shadow = false;
    theme.radius = gpui::px(6.0);
    theme.radius_lg = gpui::px(8.0);
    theme.background = background;
    theme.foreground = foreground;
    theme.muted = muted;
    theme.muted_foreground = muted_foreground;
    theme.border = border;
    theme.primary = accent;
    theme.primary_hover = primary_hover;
    theme.primary_active = primary_active;
    theme.primary_foreground = primary_foreground;
    theme.button_primary = theme.primary;
    theme.button_primary_hover = theme.primary_hover;
    theme.button_primary_active = theme.primary_active;
    theme.button_primary_foreground = theme.primary_foreground;
    // One raised-card tone for every rest-state control surface — sidebar and
    // settings cards, secondary buttons/pills, and the segmented tab track.
    theme.secondary = card_surface;
    theme.secondary_hover = hover_surface;
    theme.secondary_foreground = foreground;
    theme.secondary_active = control_hover;
    theme.group_box = card_surface;
    theme.group_box_foreground = foreground;
    theme.accent = accent_bg;
    theme.accent_foreground = foreground;
    theme.input = input;
    theme.caret = accent;
    theme.ring = accent;
    theme.selection = accent.opacity(if is_dark { 0.28 } else { 0.20 });
    let highlight_style = std::sync::Arc::make_mut(&mut theme.highlight_theme)
        .style
        .clone();
    let mut highlight_style = highlight_style;
    highlight_style.editor_background = Some(background);
    highlight_style.editor_active_line = Some(if is_dark {
        raw_color(0x000000).opacity(0.20)
    } else {
        raw_color(0x000000).opacity(0.055)
    });
    highlight_style.editor_line_number = Some(if is_dark {
        raw_color(0xFFFFFF).opacity(0.32)
    } else {
        raw_color(0x000000).opacity(0.34)
    });
    highlight_style.editor_active_line_number = Some(foreground);
    std::sync::Arc::make_mut(&mut theme.highlight_theme).style = highlight_style;
    theme.danger = danger;
    theme.danger_hover = danger.mix(theme.transparent, 0.22);
    theme.danger_active = danger.mix(theme.transparent, 0.34);
    theme.danger_foreground = primary_foreground;
    theme.warning = warning;
    theme.warning_hover = warning.mix(theme.transparent, 0.22);
    theme.warning_active = warning.mix(theme.transparent, 0.34);
    theme.warning_foreground = primary_foreground;
    theme.success = success;
    theme.success_hover = success.mix(theme.transparent, 0.22);
    theme.success_active = success.mix(theme.transparent, 0.34);
    theme.success_foreground = primary_foreground;
    theme.info = info;
    theme.info_hover = info.mix(theme.transparent, 0.22);
    theme.info_active = info.mix(theme.transparent, 0.34);
    theme.info_foreground = primary_foreground;
    theme.link = if is_dark {
        mix_towards(accent_color, color(0xFFFFFF), 0.34)
    } else {
        accent_color
    };
    theme.link_hover = if is_dark {
        mix_towards(accent_color, color(0xFFFFFF), 0.50)
    } else {
        mix_towards(accent_color, color(0x000000), 0.16)
    };
    theme.link_active = if is_dark {
        accent_color
    } else {
        mix_towards(accent_color, color(0x000000), 0.28)
    };
    theme.popover = popover;
    theme.popover_foreground = foreground;
    theme.drop_target = accent.opacity(0.16);
    theme.drag_border = accent.opacity(0.50);
    theme.tiles = muted;
    theme.title_bar = title_bar;
    theme.title_bar_border = border;
    theme.tab = tab;
    theme.tab_active = accent;
    theme.tab_active_foreground = primary_foreground;
    theme.tab_bar = tab_bar;
    theme.tab_bar_segmented = card_surface;
    theme.tab_foreground = muted_foreground;
    theme.colors.list = background;
    theme.list_hover = hover_surface;
    theme.list_active = accent_bg;
    theme.list_active_border = accent.opacity(if is_dark { 0.46 } else { 0.36 });
    // Flat selection: filled row without the border ring (list + table rows).
    theme.list.active_highlight = false;
    theme.list_head = header;
    theme.list_even = background;
    theme.table = background;
    theme.table_hover = hover_surface;
    theme.table_active = accent_bg;
    theme.table_active_border = accent.opacity(if is_dark { 0.46 } else { 0.36 });
    theme.table_even = background;
    theme.table_head = header;
    theme.table_head_foreground = muted_foreground;
    theme.table_foot = header;
    theme.table_foot_foreground = muted_foreground;
    theme.table_row_border = border;
    theme.switch = control_hover;
    theme.switch_thumb = background;
    theme.scrollbar = sidebar.opacity(0.0);
    theme.scrollbar_thumb = scrollbar_thumb;
    theme.scrollbar_thumb_hover = muted_foreground;
    theme.sidebar = sidebar;
    theme.sidebar_foreground = foreground;
    theme.sidebar_border = border;
    theme.sidebar_accent = accent_bg;
    theme.sidebar_accent_foreground = foreground;
    theme.sidebar_primary = accent;
    theme.sidebar_primary_foreground = primary_foreground;
    theme.skeleton = control_bg;
    theme.accordion = muted;
    theme.accordion_hover = hover_surface;
    theme.overlay = overlay;
    theme.window_border = border;
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODUX_DARK_THEMES: [&str; 8] = [
        "Codux Dark",
        "Deep Ocean",
        "Arctic Night",
        "Forest Night",
        "Ember",
        "Amethyst Dusk",
        "Rose Noir",
        "Carbon",
    ];
    const CODUX_LIGHT_THEMES: [&str; 8] = [
        "Codux Light",
        "Glacier",
        "Morning Mist",
        "Matcha",
        "Ivory",
        "Lavender",
        "Rosewater",
        "Sandstone",
    ];

    #[test]
    fn codux_collection_has_app_and_terminal_palettes() {
        for (names, expect_light) in [(CODUX_DARK_THEMES, false), (CODUX_LIGHT_THEMES, true)] {
            for name in names {
                let terminal = terminal_theme_palette(name);
                assert_eq!(terminal.is_light, expect_light, "{name}");
                assert_ne!(terminal.background, terminal.foreground, "{name}");
                let app = app_theme_palette(&normalize_theme_name(name))
                    .unwrap_or_else(|| panic!("{name} missing app palette"));
                // Text must contrast its ground in the designed direction.
                let luma = |hex: u32| {
                    0.2126 * ((hex >> 16) & 0xFF) as f32
                        + 0.7152 * ((hex >> 8) & 0xFF) as f32
                        + 0.0722 * (hex & 0xFF) as f32
                };
                let ground = luma(app.ground);
                assert_eq!(ground > 128.0, expect_light, "{name} ground");
                assert!((luma(app.text) - ground).abs() > 90.0, "{name} contrast");
            }
        }
    }

    #[test]
    fn auto_theme_resolves_codux_flagships() {
        let dark = terminal_theme_palette_for_appearance("Auto", WindowAppearance::Dark);
        assert_eq!(
            dark.background,
            terminal_theme_palette("Codux Dark").background
        );
        let light = terminal_theme_palette_for_appearance("Auto", WindowAppearance::Light);
        assert!(light.is_light);
        // Legacy persisted names fall back to the flagship dark palette.
        let fallback = app_theme_palette_for_appearance("Dracula", WindowAppearance::Dark);
        assert_eq!(
            fallback.ground,
            app_theme_palette("codux dark").expect("flagship").ground
        );
    }
}
