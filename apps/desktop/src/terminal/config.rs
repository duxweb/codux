#[derive(Clone, Debug)]
pub struct TerminalConfig {
    pub cols: usize,
    pub rows: usize,
    pub font_family: String,
    pub font_size: Pixels,
    pub scrollback: usize,
    pub line_height_multiplier: f32,
    pub padding: Edges<Pixels>,
    pub colors: ColorPalette,
    pub paste_images_as_paths: bool,
}

pub fn terminal_config() -> TerminalConfig {
    let colors = ColorPalette::builder()
        .background(0x11, 0x14, 0x1A)
        .foreground(0xD6, 0xDA, 0xE2)
        .cursor(0xF3, 0xC9, 0x6B)
        .black(0x1A, 0x1D, 0x24)
        .red(0xF2, 0x72, 0x72)
        .green(0x7D, 0xD8, 0x92)
        .yellow(0xE8, 0xC6, 0x6A)
        .blue(0x7A, 0xB8, 0xFF)
        .magenta(0xD6, 0x8A, 0xFF)
        .cyan(0x66, 0xD9, 0xE8)
        .white(0xD6, 0xDA, 0xE2)
        .bright_black(0x5C, 0x65, 0x73)
        .bright_red(0xFF, 0x9B, 0x9B)
        .bright_green(0xA8, 0xEE, 0xB7)
        .bright_yellow(0xF4, 0xD9, 0x86)
        .bright_blue(0xA6, 0xD0, 0xFF)
        .bright_magenta(0xE6, 0xB3, 0xFF)
        .bright_cyan(0x9E, 0xF0, 0xF5)
        .bright_white(0xFF, 0xFF, 0xFF)
        .build();

    TerminalConfig {
        font_family: default_terminal_font_family().into(),
        font_size: px(14.0),
        cols: 100,
        rows: 32,
        scrollback: 10_000,
        line_height_multiplier: DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
        padding: Edges::all(px(10.0)),
        colors,
        paste_images_as_paths: true,
    }
}

pub fn terminal_config_with_font_family(font_family: &str) -> TerminalConfig {
    let mut config = terminal_config();
    let font_family = font_family.trim();
    if !font_family.is_empty() {
        config.font_family = font_family.to_string();
    }
    config
}

fn terminal_text_width(text: &str) -> usize {
    text.chars()
        .map(|ch| {
            if ch.is_ascii()
                || matches!(
                    ch as u32,
                    0x0300..=0x036F
                        | 0x1AB0..=0x1AFF
                        | 0x1DC0..=0x1DFF
                        | 0x20D0..=0x20FF
                        | 0xFE20..=0xFE2F
                )
            {
                1
            } else {
                2
            }
        })
        .sum::<usize>()
        .max(1)
}

fn terminal_grid_dimension(available: f32, cell: f32, minimum: usize) -> usize {
    if !available.is_finite() || !cell.is_finite() || cell <= 0.0 {
        return minimum;
    }
    (available / cell).next_up().floor().max(minimum as f32) as usize
}

fn default_terminal_font_family() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "Liberation Mono"
    }
}

const DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER: f32 = 1.45;
const TERMINAL_SCROLL_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const TERMINAL_OUTPUT_FRAME_INTERVAL: Duration = Duration::from_millis(4);
const TERMINAL_INITIAL_LAYOUT_WAIT: Duration = Duration::from_millis(120);
const TERMINAL_ROW_CACHE_LIMIT: usize = 4096;
static TERMINAL_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
