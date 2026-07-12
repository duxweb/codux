use super::super::*;
use super::fixtures::*;
use std::path::{Path, PathBuf};

#[test]
fn pty_config_carries_theme_osc_env_for_wrappers() {
    let mut terminal_config = terminal_config();
    terminal_config.colors = ColorPalette::builder()
        .background(0xfa, 0xfb, 0xfc)
        .foreground(0x2a, 0x31, 0x40)
        .build();
    let config = terminal_pty_config_with_view(TerminalPtyConfig::default(), &terminal_config);
    let env = config.env.expect("env injected");
    assert_eq!(
        env.get("DMUX_TERMINAL_OSC_BG").map(String::as_str),
        Some("rgb:fafa/fbfb/fcfc")
    );
    assert_eq!(
        env.get("DMUX_TERMINAL_OSC_FG").map(String::as_str),
        Some("rgb:2a2a/3131/4040")
    );
}
#[test]
fn inverse_cells_swap_foreground_and_background_colors() {
    let renderer = TerminalRenderer::new(
        default_terminal_font_family().to_string(),
        px(14.0),
        DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
        ColorPalette::default(),
    );
    let normal_cell = test_cell(
        TerminalScreenColor::Default,
        TerminalScreenColor::Default,
        false,
        false,
    );
    let inverse_cell = test_cell(
        TerminalScreenColor::Default,
        TerminalScreenColor::Default,
        false,
        true,
    );

    let normal = renderer.cell_render_colors(&normal_cell);
    let inverse = renderer.cell_render_colors(&inverse_cell);

    assert_eq!(inverse.0, normal.1);
    assert_eq!(inverse.1, normal.0);
}
#[test]
fn default_terminal_line_height_matches_renderer_cell_height() {
    let config = terminal_config();
    assert_eq!(
        config.line_height_multiplier,
        DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER
    );

    let renderer = TerminalRenderer::new(
        config.font_family,
        config.font_size,
        config.line_height_multiplier,
        config.colors,
    );
    assert_eq!(
        renderer.cell_height,
        config.font_size * DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER
    );
    assert!(config.paste_images_as_paths);
}
#[test]
fn terminal_builtin_graphics_cover_box_block_and_powerline_ranges() {
    assert!(terminal_builtin_graphic(0x2502).is_some());
    assert!(terminal_builtin_graphic(0x2588).is_some());
    assert!(terminal_builtin_graphic(0x2595).is_some());
    assert!(terminal_builtin_graphic('a' as u32).is_none());
    assert_eq!(terminal_cell_codepoint("│"), Some(0x2502));
    assert_eq!(terminal_cell_codepoint("ab"), None);
    // Powerline separators are cell-exact vectors, not symbol-font glyphs.
    assert!(terminal_builtin_graphic(0xE0B0).is_some());
    assert!(terminal_builtin_graphic(0xE0B4).is_some());
    assert!(terminal_builtin_graphic(0xE0BF).is_some());
    assert!(terminal_builtin_graphic(0xE0C0).is_none());
    assert!(terminal_builtin_graphic(0xF017).is_none());
    // Braille (btop graphs) and legacy-computing sextants.
    assert!(terminal_builtin_graphic(0x2800).is_some());
    assert!(terminal_builtin_graphic(0x28FF).is_some());
    assert!(terminal_builtin_graphic(0x1FB00).is_some());
    assert!(terminal_builtin_graphic(0x1FB3B).is_some());
    assert!(terminal_builtin_graphic(0x1FB3C).is_none());
    // Sextant fill indexes skip the half/full-block gaps.
    assert!(matches!(
        terminal_builtin_graphic(0x1FB00),
        Some(TerminalCellGraphic::Sextant(1))
    ));
    assert!(matches!(
        terminal_builtin_graphic(0x1FB14),
        Some(TerminalCellGraphic::Sextant(22))
    ));
    assert!(matches!(
        terminal_builtin_graphic(0x1FB3B),
        Some(TerminalCellGraphic::Sextant(62))
    ));
}
#[test]
fn terminal_clipboard_image_payload_detection_filters_data_and_html() {
    assert!(clipboard_text_looks_like_image_payload(
        "data:image/png;base64,abc"
    ));
    assert!(clipboard_text_looks_like_image_payload(
        "<img src=\"data:image/png;base64,abc\">"
    ));
    assert!(!clipboard_text_looks_like_image_payload("/tmp/image.png"));
}
#[test]
fn terminal_clipboard_plain_text_skips_rich_format_reading() {
    assert_eq!(
        terminal_clipboard_text_preference(Some("echo ready".to_string()), true),
        TerminalClipboardTextPreference::Text("echo ready".to_string())
    );
    assert_eq!(
        terminal_clipboard_text_preference(Some("echo ready".to_string()), false),
        TerminalClipboardTextPreference::Text("echo ready".to_string())
    );
}
#[test]
fn terminal_clipboard_image_payload_uses_rich_format_reading() {
    assert_eq!(
        terminal_clipboard_text_preference(Some("data:image/png;base64,abc".to_string()), true),
        TerminalClipboardTextPreference::RichClipboard
    );
    assert_eq!(
        terminal_clipboard_text_preference(None, true),
        TerminalClipboardTextPreference::RichClipboard
    );
    assert_eq!(
        terminal_clipboard_text_preference(None, false),
        TerminalClipboardTextPreference::None
    );
}
#[test]
fn terminal_path_input_quotes_spaces() {
    assert_eq!(
        terminal_path_input(Path::new("/tmp/codux image.png")),
        "'/tmp/codux image.png'"
    );
    assert_eq!(
        terminal_path_input(Path::new("/tmp/codux-image.png")),
        "/tmp/codux-image.png"
    );
    assert_eq!(terminal_clipboard_image_extension(ImageFormat::Jpeg), "jpg");
}
#[test]
fn terminal_paths_input_joins_quoted_paths_with_trailing_space() {
    let paths = vec![
        PathBuf::from("/tmp/codux-image.png"),
        PathBuf::from("/tmp/codux image.png"),
    ];

    assert_eq!(
        terminal_paths_input(&paths),
        Some("/tmp/codux-image.png '/tmp/codux image.png' ".to_string())
    );
    assert_eq!(terminal_paths_input(&[]), None);
}
#[test]
fn bold_ansi_foreground_uses_bright_color() {
    let renderer = TerminalRenderer::new(
        default_terminal_font_family().to_string(),
        px(14.0),
        DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
        ColorPalette::default(),
    );
    let cell = test_cell(
        TerminalScreenColor::Indexed { index: 4 },
        TerminalScreenColor::Default,
        true,
        false,
    );

    let (fg, _) = renderer.cell_render_colors(&cell);
    assert_eq!(
        fg,
        renderer
            .palette
            .resolve_fg(&TerminalScreenColor::Indexed { index: 12 }, false, false)
    );
}
#[test]
fn default_colors_use_current_palette_values() {
    let palette = ColorPalette::builder()
        .background(0xee, 0xee, 0xee)
        .foreground(0x11, 0x11, 0x11)
        .cursor(0x22, 0x22, 0x22)
        .build();

    assert_eq!(
        palette.resolve_bg(&TerminalScreenColor::Default),
        palette.background()
    );
    assert_eq!(
        palette.resolve_fg(&TerminalScreenColor::Default, false, false),
        palette.foreground()
    );
}

#[test]
fn dim_light_text_stays_readable_on_light_background() {
    let renderer = TerminalRenderer::new(
        default_terminal_font_family().to_string(),
        px(14.0),
        DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
        ColorPalette::builder()
            .background(0xfa, 0xfb, 0xfc)
            .foreground(0x2a, 0x31, 0x40)
            .build(),
    );
    let mut cell = test_cell(
        TerminalScreenColor::Rgb {
            r: 0xff,
            g: 0xff,
            b: 0xff,
        },
        TerminalScreenColor::Default,
        false,
        false,
    );
    cell.dim = true;

    let (foreground, background) = renderer.cell_render_colors(&cell);

    assert!(
        contrast_ratio(hsla_to_rgb(foreground), hsla_to_rgb(background)) >= 3.0,
        "dim truecolor text should remain readable on light terminal themes"
    );
}

#[test]
fn inverse_bold_only_brightens_final_foreground() {
    let renderer = TerminalRenderer::new(
        default_terminal_font_family().to_string(),
        px(14.0),
        DEFAULT_TERMINAL_LINE_HEIGHT_MULTIPLIER,
        ColorPalette::default(),
    );
    let cell = test_cell(
        TerminalScreenColor::Indexed { index: 4 },
        TerminalScreenColor::Indexed { index: 1 },
        true,
        true,
    );

    let (fg, bg) = renderer.cell_render_colors(&cell);
    assert_eq!(
        fg,
        renderer
            .palette
            .resolve_fg(&TerminalScreenColor::Indexed { index: 9 }, false, false)
    );
    assert_eq!(
        bg,
        renderer
            .palette
            .resolve_fg(&TerminalScreenColor::Indexed { index: 4 }, false, false)
    );
}
#[test]
fn palette_resolves_configured_colors() {
    let palette = ColorPalette::builder()
        .background(0x28, 0x2A, 0x36)
        .foreground(0xF8, 0xF8, 0xF2)
        .cursor(0xF8, 0xF8, 0xF2)
        .selection(0x44, 0x47, 0x5A)
        .black(0x21, 0x22, 0x2C)
        .bright_black(0x62, 0x72, 0xA4)
        .build();

    assert_eq!(
        hsla_to_rgb(palette.resolve_bg(&TerminalScreenColor::Default)),
        TerminalRgb {
            r: 0x28,
            g: 0x2A,
            b: 0x36
        }
    );
    assert_eq!(
        hsla_to_rgb(palette.resolve_fg(&TerminalScreenColor::Default, false, false)),
        TerminalRgb {
            r: 0xF8,
            g: 0xF8,
            b: 0xF2
        }
    );
    assert_eq!(
        hsla_to_rgb(palette.resolve_fg(&TerminalScreenColor::Indexed { index: 0 }, false, false)),
        TerminalRgb {
            r: 0x21,
            g: 0x22,
            b: 0x2C
        }
    );
    assert_eq!(
        hsla_to_rgb(palette.resolve_fg(&TerminalScreenColor::Indexed { index: 8 }, false, false)),
        TerminalRgb {
            r: 0x62,
            g: 0x72,
            b: 0xA4
        }
    );
    assert_eq!(
        hsla_to_rgb(palette.resolve_fg(&TerminalScreenColor::Default, true, false)),
        TerminalRgb {
            r: 0xF8,
            g: 0xF8,
            b: 0xF2
        }
    );
    assert_eq!(
        palette.resolve_fg(&TerminalScreenColor::Default, false, true),
        dim_color(
            rgb_to_hsla(TerminalRgb {
                r: 0xF8,
                g: 0xF8,
                b: 0xF2
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x28,
                g: 0x2A,
                b: 0x36
            })
        )
    );
    assert_eq!(
        hsla_to_rgb(palette.resolve_fg(&TerminalScreenColor::Indexed { index: 255 }, false, false)),
        TerminalRgb {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE
        }
    );
}
