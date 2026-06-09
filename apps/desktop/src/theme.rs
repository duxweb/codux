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
        CoduxTitlebarKind::Main => Some(point(px(12.0), px(15.0))),
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
    mut window: Option<&mut Window>,
    cx: &mut App,
) {
    let appearance = window
        .as_ref()
        .map(|window| window.appearance())
        .unwrap_or_else(|| cx.window_appearance());
    let terminal = terminal_theme_palette_for_appearance(theme_name, appearance);
    let mode = if terminal.is_light {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    Theme::change(mode, window.as_deref_mut(), cx);

    configure_component_theme(cx, terminal, theme_color_value(theme_color));
    cx.refresh_windows();
    if let Some(window) = window {
        window.refresh();
    }
}

pub fn apply_component_theme_for_appearance(
    theme_name: &str,
    theme_color: &str,
    appearance: WindowAppearance,
    mut window: Option<&mut Window>,
    cx: &mut App,
) {
    let terminal = terminal_theme_palette_for_appearance(theme_name, appearance);
    let mode = if terminal.is_light {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    Theme::change(mode, window.as_deref_mut(), cx);

    configure_component_theme(cx, terminal, theme_color_value(theme_color));
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

impl TerminalThemePalette {
    fn auto(is_light: bool) -> Self {
        if is_light {
            return terminal_theme_palette("2017 Light");
        }
        terminal_theme_palette("2017 Dark")
    }

    #[allow(clippy::too_many_arguments)]
    fn from_colors(
        is_light: bool,
        background: u32,
        foreground: u32,
        cursor: u32,
        selection: u32,
        black: u32,
        red: u32,
        green: u32,
        yellow: u32,
        blue: u32,
        magenta: u32,
        cyan: u32,
        white: u32,
        bright_black: u32,
        bright_red: u32,
        bright_green: u32,
        bright_yellow: u32,
        bright_blue: u32,
        bright_magenta: u32,
        bright_cyan: u32,
        bright_white: u32,
    ) -> Self {
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
        "2017 dark" => TerminalThemePalette::from_colors(
            false, 0x1E1E1E, 0xD4D4D4, 0xD4D4D4, 0x264F78, 0x000000, 0xCD3131, 0x0DBC79, 0xE5E510,
            0x2472C8, 0xBC3FBC, 0x11A8CD, 0xE5E5E5, 0x666666, 0xF14C4C, 0x23D18B, 0xF5F543,
            0x3B8EEA, 0xD670D6, 0x29B8DB, 0xFFFFFF,
        ),
        "github dark" => TerminalThemePalette::from_colors(
            false, 0x24292E, 0xE1E4E8, 0xC8E1FF, 0x3392FF, 0x586069, 0xEA4A5A, 0x34D058, 0xFFEA7F,
            0x2188FF, 0xB392F0, 0x39C5CF, 0xD1D5DA, 0x959DA5, 0xF97583, 0x85E89D, 0xFFEA7F,
            0x79B8FF, 0xB392F0, 0x56D4DD, 0xFAFBFC,
        ),
        "one dark pro" => TerminalThemePalette::from_colors(
            false, 0x282C34, 0xABB2BF, 0x528BFF, 0x677696, 0x3F4451, 0xE05561, 0x8CC265, 0xD18F52,
            0x4AA5F0, 0xC162DE, 0x42B3C2, 0xD7DAE0, 0x4F5666, 0xFF616E, 0xA5E075, 0xF0A45D,
            0x4DC4FF, 0xDE73FF, 0x4CD1E0, 0xE6E6E6,
        ),
        "dracula" => TerminalThemePalette::from_colors(
            false, 0x282A36, 0xF8F8F2, 0xF8F8F2, 0x44475A, 0x21222C, 0xFF5555, 0x50FA7B, 0xF1FA8C,
            0xBD93F9, 0xFF79C6, 0x8BE9FD, 0xF8F8F2, 0x6272A4, 0xFF6E6E, 0x69FF94, 0xFFFFA5,
            0xD6ACFF, 0xFF92DF, 0xA4FFFF, 0xFFFFFF,
        ),
        "atom one dark" => TerminalThemePalette::from_colors(
            false, 0x282C34, 0xABB2BF, 0x528BFF, 0x3E4451, 0x000000, 0xCD3131, 0x0DBC79, 0xE5E510,
            0x2472C8, 0xBC3FBC, 0x11A8CD, 0xE5E5E5, 0x666666, 0xF14C4C, 0x23D18B, 0xF5F543,
            0x3B8EEA, 0xD670D6, 0x29B8DB, 0xFFFFFF,
        ),
        "material theme" => TerminalThemePalette::from_colors(
            false, 0x263238, 0xEEFFFF, 0xFFCC00, 0x80CBC4, 0x000000, 0xF07178, 0xC3E88D, 0xFFCB6B,
            0x82AAFF, 0xC792EA, 0x89DDFF, 0xFFFFFF, 0x546E7A, 0xF07178, 0xC3E88D, 0xFFCB6B,
            0x82AAFF, 0xC792EA, 0x89DDFF, 0xFFFFFF,
        ),
        "ayu dark" => TerminalThemePalette::from_colors(
            false, 0x0D1017, 0xBFBDB6, 0xE6B450, 0x3388FF, 0x1B1F29, 0xF06B73, 0x70BF56, 0xFDB04C,
            0x4FBFFF, 0xD0A1FF, 0x93E2C8, 0xC7C7C7, 0x686868, 0xF07178, 0xAAD94C, 0xFFB454,
            0x59C2FF, 0xD2A6FF, 0x95E6CB, 0xFFFFFF,
        ),
        "monokai pro" => TerminalThemePalette::from_colors(
            false, 0x2D2A2E, 0xFCFCFA, 0xFCFCFA, 0xC1C0C0, 0x403E41, 0xFF6188, 0xA9DC76, 0xFFD866,
            0xFC9867, 0xAB9DF2, 0x78DCE8, 0xFCFCFA, 0x727072, 0xFF6188, 0xA9DC76, 0xFFD866,
            0xFC9867, 0xAB9DF2, 0x78DCE8, 0xFCFCFA,
        ),
        "winter is coming dark blue" => TerminalThemePalette::from_colors(
            false, 0x011627, 0xA7DBF7, 0x219FD5, 0x103362, 0x011627, 0xCD3131, 0x0DBC79, 0xE5E510,
            0x2472C8, 0xBC3FBC, 0x11A8CD, 0xE5E5E5, 0x666666, 0xF14C4C, 0x23D18B, 0xF5F543,
            0x3B8EEA, 0xD670D6, 0x29B8DB, 0xFFFFFF,
        ),
        "night owl" => TerminalThemePalette::from_colors(
            false, 0x011627, 0xD6DEEB, 0x80A4C2, 0x1D3B53, 0x011627, 0xEF5350, 0x22DA6E, 0xC5E478,
            0x82AAFF, 0xC792EA, 0x21C7A8, 0xFFFFFF, 0x575656, 0xEF5350, 0x22DA6E, 0xFFEB95,
            0x82AAFF, 0xC792EA, 0x7FDBCA, 0xFFFFFF,
        ),
        "one monokai" => TerminalThemePalette::from_colors(
            false, 0x282C34, 0xD4D4D4, 0xF8F8F0, 0x3E4451, 0x2D3139, 0xE06C75, 0x98C379, 0xE5C07B,
            0x528BFF, 0xC678DD, 0x56B6C2, 0xD7DAE0, 0x7F848E, 0xF44747, 0x98C379, 0xE5C07B,
            0x528BFF, 0x7E0097, 0x56B6C2, 0xD7DAE0,
        ),
        "tokyo night" => TerminalThemePalette::from_colors(
            false, 0x1A1B26, 0xA9B1D6, 0xC0CAF5, 0x515C7E, 0x363B54, 0xF7768E, 0x73DACA, 0xE0AF68,
            0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0x787C99, 0x363B54, 0xF7768E, 0x73DACA, 0xE0AF68,
            0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0xACB0D0,
        ),
        "palenight" => TerminalThemePalette::from_colors(
            false, 0x292D3E, 0xBFC7D5, 0x7E57C2, 0x7580B8, 0x676E95, 0xFF5572, 0xA9C77D, 0xFFCB6B,
            0x82AAFF, 0xC792EA, 0x89DDFF, 0xFFFFFF, 0x676E95, 0xFF5572, 0xC3E88D, 0xFFCB6B,
            0x82AAFF, 0xC792EA, 0x89DDFF, 0xFFFFFF,
        ),
        "synthwave '84" => TerminalThemePalette::from_colors(
            false, 0x262335, 0xD4D4D4, 0xF97E72, 0xFFFFFF, 0x000000, 0xFE4450, 0x72F1B8, 0xF3E70F,
            0x03EDF9, 0xFF7EDB, 0x03EDF9, 0xE5E5E5, 0x666666, 0xFE4450, 0x72F1B8, 0xFEDE5D,
            0x03EDF9, 0xFF7EDB, 0x03EDF9, 0xFFFFFF,
        ),
        "shades of purple" => TerminalThemePalette::from_colors(
            false, 0x2D2B55, 0xFFFFFF, 0xFAD000, 0xB362FF, 0x000000, 0xEC3A37, 0x3AD900, 0xFAD000,
            0x7857FE, 0xFF2C70, 0x80FCFF, 0xFFFFFF, 0x5C5C61, 0xEC3A37, 0x3AD900, 0xFAD000,
            0x6943FF, 0xFB94FF, 0x80FCFF, 0xFFFFFF,
        ),
        "2017 light" => TerminalThemePalette::from_colors(
            true, 0xFFFFFF, 0x000000, 0x000000, 0xADD6FF, 0x000000, 0xCD3131, 0x00BC00, 0x949800,
            0x0451A5, 0xBC05BC, 0x0598BC, 0x555555, 0x666666, 0xCD3131, 0x14CE14, 0xB5BA00,
            0x0451A5, 0xBC05BC, 0x0598BC, 0xA5A5A5,
        ),
        "powershell ise" => TerminalThemePalette::from_colors(
            true, 0xFFFFFF, 0x000000, 0x000000, 0x94C6F7, 0x000000, 0xCD3131, 0x00BC00, 0x949800,
            0x0451A5, 0xBC05BC, 0x0598BC, 0x555555, 0x666666, 0xCD3131, 0x14CE14, 0xB5BA00,
            0x0451A5, 0xBC05BC, 0x0598BC, 0xA5A5A5,
        ),
        "github light" => TerminalThemePalette::from_colors(
            true, 0xFFFFFF, 0x24292E, 0x044289, 0x0366D6, 0x24292E, 0xD73A49, 0x28A745, 0xDBAB09,
            0x0366D6, 0x5A32A3, 0x1B7C83, 0x6A737D, 0x959DA5, 0xCB2431, 0x22863A, 0xB08800,
            0x005CC5, 0x5A32A3, 0x3192AA, 0xD1D5DA,
        ),
        "material theme lighter" => TerminalThemePalette::from_colors(
            true, 0xFAFAFA, 0x90A4AE, 0x272727, 0x80CBC4, 0x000000, 0xE53935, 0x91B859, 0xE2931D,
            0x6182B8, 0x9C3EDA, 0x39ADB5, 0xFFFFFF, 0x90A4AE, 0xE53935, 0x91B859, 0xE2931D,
            0x6182B8, 0x9C3EDA, 0x39ADB5, 0xFFFFFF,
        ),
        "ayu light" => TerminalThemePalette::from_colors(
            true, 0xF8F9FA, 0x5C6166, 0xF29718, 0x035BD6, 0x000000, 0xF06B6C, 0x6CBF43, 0xE7A100,
            0x21A1E2, 0xA176CB, 0x4ABC96, 0xC7C7C7, 0x686868, 0xF07171, 0x86B300, 0xEBA400,
            0x22A4E6, 0xA37ACC, 0x4CBF99, 0xD1D1D1,
        ),
        "monokai pro light" => TerminalThemePalette::from_colors(
            true, 0xFAF4F2, 0x29242A, 0x29242A, 0x706B6E, 0xD3CDCC, 0xE14775, 0x269D69, 0xCC7A0A,
            0xE16032, 0x7058BE, 0x1C8CA8, 0x29242A, 0xA59FA0, 0xE14775, 0x269D69, 0xCC7A0A,
            0xE16032, 0x7058BE, 0x1C8CA8, 0x29242A,
        ),
        "winter is coming light" => TerminalThemePalette::from_colors(
            true, 0xFFFFFF, 0x236EBF, 0x4FB4D8, 0xCEE1F0, 0x011627, 0xCD3131, 0x00BC00, 0x949800,
            0x0451A5, 0xBC05BC, 0x0598BC, 0x555555, 0x666666, 0xCD3131, 0x14CE14, 0xB5BA00,
            0x0451A5, 0xBC05BC, 0x0598BC, 0xA5A5A5,
        ),
        "night owl light" => TerminalThemePalette::from_colors(
            true, 0xFBFBFB, 0x403F53, 0x90A7B2, 0xE0E0E0, 0x403F53, 0xDE3D3B, 0x08916A, 0xE0AF02,
            0x288ED7, 0xD6438A, 0x2AA298, 0x93A1A1, 0x403F53, 0xDE3D3B, 0x08916A, 0xDAAA01,
            0x288ED7, 0xD6438A, 0x2AA298, 0x93A1A1,
        ),
        "tokyo night light" => TerminalThemePalette::from_colors(
            true, 0xE6E7ED, 0x343B59, 0x363C4D, 0xACB0BF, 0x343B58, 0x8C4351, 0x33635C, 0x8F5E15,
            0x2959AA, 0x7B43BA, 0x006C86, 0x707280, 0x343B58, 0x8C4351, 0x33635C, 0x8F5E15,
            0x2959AA, 0x7B43BA, 0x006C86, 0x707280,
        ),
        "atom one light" => TerminalThemePalette::from_colors(
            true, 0xFAFAFA, 0x383A42, 0x526FFF, 0xE5E5E6, 0x000000, 0xCD3131, 0x00BC00, 0x949800,
            0x0451A5, 0xBC05BC, 0x0598BC, 0x555555, 0x666666, 0xCD3131, 0x14CE14, 0xB5BA00,
            0x0451A5, 0xBC05BC, 0x0598BC, 0xA5A5A5,
        ),
        "noctis hibernus" => TerminalThemePalette::from_colors(
            true, 0xF4F6F6, 0x005661, 0x0092A8, 0xADE2EB, 0x003B42, 0xE34E1C, 0x00B368, 0xF49725,
            0x0094F0, 0xFF5792, 0x00BDD6, 0x8CA6A6, 0x004D57, 0xFF4000, 0x00D17A, 0xFF8C00,
            0x0FA3FF, 0xFF6B9F, 0x00CBE6, 0xBBC3C4,
        ),
        "catppuccin latte" => TerminalThemePalette::from_colors(
            true, 0xEFF1F5, 0x4C4F69, 0xDC8A78, 0x7C7F93, 0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D,
            0x1E66F5, 0xEA76CB, 0x179299, 0xACB0BE, 0x6C6F85, 0xDE293E, 0x49AF3D, 0xEEA02D,
            0x456EFF, 0xFE85D8, 0x2D9FA8, 0xBCC0CC,
        ),
        "gruvbox light medium" => TerminalThemePalette::from_colors(
            true, 0xFBF1C7, 0x3C3836, 0x3C3836, 0x689D6A, 0xEBDBB2, 0xCC241D, 0x98971A, 0xD79921,
            0x458588, 0xB16286, 0x689D6A, 0x7C6F64, 0x928374, 0x9D0006, 0x79740E, 0xB57614,
            0x076678, 0x8F3F71, 0x427B58, 0x3C3836,
        ),
        "eva light" => TerminalThemePalette::from_colors(
            true, 0xEBEEF5, 0x5D5D5F, 0xFC8357, 0x0065FF, 0xEBEEF5, 0xEC0000, 0x44C145, 0xF0AA0B,
            0x4480F4, 0xC838C6, 0x00BEC4, 0x5D5D5F, 0xAAADB4, 0xF14C4C, 0x44C145, 0xFF6D12,
            0x4D91F8, 0xEF8ED8, 0x00BEC4, 0x888888,
        ),
        "spinel light" => TerminalThemePalette::from_colors(
            true, 0xE3E2EC, 0x595959, 0x595959, 0xD8D8DB, 0x000000, 0xCD3131, 0x00BC00, 0x949800,
            0x0451A5, 0xBC05BC, 0x0598BC, 0x555555, 0x666666, 0xCD3131, 0x14CE14, 0xB5BA00,
            0x0451A5, 0xBC05BC, 0x0598BC, 0xA5A5A5,
        ),
        _ => TerminalThemePalette::auto(false),
    }
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

fn configure_component_theme(cx: &mut App, terminal: TerminalThemePalette, accent_hex: u32) {
    let is_dark = !terminal.is_light;
    let theme = Theme::global_mut(cx);
    let terminal_background = raw_color(terminal.background);
    let accent_color = raw_color(accent_hex);
    let (
        background,
        foreground,
        muted,
        muted_foreground,
        border,
        control_bg,
        control_hover,
        input,
        accent_bg,
        accent,
        primary_hover,
        primary_active,
        primary_foreground,
        popover,
        sidebar,
        header,
        row_hover,
        title_bar,
        tab,
        tab_bar,
        tab_segmented,
        scrollbar_thumb,
        overlay,
        danger,
        warning,
        success,
        info,
        task_column,
    ) = if is_dark {
        let selection = raw_color(terminal.selection);
        let current_line = raw_color(terminal.bright_black);
        let terminal_black = raw_color(terminal.black);
        let app_surface = mix_towards(terminal_background, terminal_black, 0.36);
        let column_surface = mix_towards(app_surface, terminal_background, 0.48);
        let header_surface = mix_towards(app_surface, selection, 0.10);
        (
            app_surface,
            raw_color(terminal.foreground),
            mix_towards(terminal_background, selection, 0.34),
            mix_towards(
                raw_color(terminal.muted_foreground),
                terminal_background,
                0.28,
            ),
            mix_towards(terminal_background, current_line, 0.30),
            raw_color(0xFFFFFF).opacity(0.055),
            raw_color(0xFFFFFF).opacity(0.085),
            raw_color(0xFFFFFF).opacity(0.075),
            accent_color.opacity(0.17),
            accent_color,
            mix_towards(accent_color, raw_color(0xFFFFFF), 0.18),
            mix_towards(accent_color, raw_color(0x000000), 0.16),
            raw_color(0xF6FAFF),
            mix_towards(app_surface, terminal_black, 0.18),
            app_surface,
            header_surface,
            mix_towards(terminal_background, selection, 0.58),
            app_surface,
            terminal_background,
            header_surface,
            mix_towards(column_surface, selection, 0.08),
            mix_towards(terminal_background, current_line, 0.42),
            raw_color(0x000000).opacity(0.42),
            raw_color(0xF87171),
            color(ORANGE),
            color(GREEN),
            raw_color(0x60A5FA),
            column_surface,
        )
    } else {
        (
            mix_towards(terminal_background, raw_color(0x000000), 0.035),
            raw_color(terminal.foreground),
            mix_towards(terminal_background, raw_color(0x000000), 0.035),
            raw_color(terminal.muted_foreground),
            mix_towards(terminal_background, raw_color(0x000000), 0.12),
            raw_color(0x000000).opacity(0.055),
            raw_color(0x000000).opacity(0.085),
            raw_color(0x000000).opacity(0.075),
            accent_color.opacity(0.12),
            accent_color,
            mix_towards(accent_color, raw_color(0x000000), 0.12),
            mix_towards(accent_color, raw_color(0x000000), 0.22),
            raw_color(0xFFFFFF),
            mix_towards(terminal_background, raw_color(0xFFFFFF), 0.82),
            mix_towards(terminal_background, raw_color(0x000000), 0.070),
            mix_towards(terminal_background, raw_color(0x000000), 0.055),
            mix_towards(terminal_background, raw_color(0x000000), 0.075),
            mix_towards(terminal_background, raw_color(0x000000), 0.055),
            terminal_background,
            mix_towards(terminal_background, raw_color(0x000000), 0.045),
            raw_color(0x000000).opacity(0.055),
            mix_towards(terminal_background, raw_color(0x000000), 0.26),
            raw_color(0x000000).opacity(0.28),
            raw_color(0xDC2626),
            raw_color(0xD97706),
            raw_color(0x16A34A),
            raw_color(0x2563EB),
            mix_towards(terminal_background, raw_color(0x000000), 0.045),
        )
    };
    let hover_surface = if is_dark {
        raw_color(0xFFFFFF).opacity(0.10)
    } else {
        raw_color(0x000000).opacity(0.07)
    };

    set_dynamic_color(&DYNAMIC_BG, rgba_to_u32(background));
    set_dynamic_color(&DYNAMIC_BG_ELEVATED, rgba_to_u32(popover));
    let panel_surface = if is_dark {
        mix_towards(terminal_background, raw_color(0xFFFFFF), 0.055)
    } else {
        mix_towards(terminal_background, raw_color(0x000000), 0.035)
    };
    set_dynamic_color(&DYNAMIC_BG_PANEL, rgba_to_u32(panel_surface));
    set_dynamic_color(&DYNAMIC_BG_TERMINAL, terminal.background);
    set_dynamic_color(&DYNAMIC_BG_COLUMN, rgba_to_u32(task_column));
    set_dynamic_color(&DYNAMIC_BG_HEADER, rgba_to_u32(header));
    set_dynamic_color(&DYNAMIC_BG_ROW_HOVER, rgba_to_u32(row_hover));
    set_dynamic_color(&DYNAMIC_BG_ROW_ACTIVE, rgba_to_u32(accent_bg));
    set_dynamic_color(&DYNAMIC_BORDER, rgba_to_u32(border));
    set_dynamic_color(&DYNAMIC_BORDER_SOFT, rgba_to_u32(border));
    set_dynamic_color(&DYNAMIC_TEXT, terminal.foreground);
    set_dynamic_color(&DYNAMIC_TEXT_MUTED, terminal.muted_foreground);
    set_dynamic_color(&DYNAMIC_TEXT_DIM, rgba_to_u32(muted_foreground));
    set_dynamic_color(&DYNAMIC_ACCENT, accent_hex);
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
    theme.secondary = control_bg;
    theme.secondary_hover = hover_surface;
    theme.secondary_foreground = foreground;
    theme.secondary_active = control_hover;
    theme.group_box = control_bg;
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
    theme.tab_bar_segmented = tab_segmented;
    theme.tab_foreground = muted_foreground;
    theme.colors.list = background;
    theme.list_hover = hover_surface;
    theme.list_active = accent_bg;
    theme.list_active_border = accent.opacity(if is_dark { 0.46 } else { 0.36 });
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
