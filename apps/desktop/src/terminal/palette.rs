#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TerminalRgb {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Debug, Clone)]
pub struct ColorPalette {
    ansi_colors: [Hsla; 16],
    extended_colors: [Hsla; 256],
    foreground: Hsla,
    background: Hsla,
    cursor: Hsla,
    selection: Hsla,
}

impl Default for ColorPalette {
    fn default() -> Self {
        let ansi_colors = [
            rgb_to_hsla(TerminalRgb {
                r: 0x00,
                g: 0x00,
                b: 0x00,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0xcc,
                g: 0x00,
                b: 0x00,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x4e,
                g: 0x9a,
                b: 0x06,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0xc4,
                g: 0xa0,
                b: 0x00,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x34,
                g: 0x65,
                b: 0xa4,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x75,
                g: 0x50,
                b: 0x7b,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x06,
                g: 0x98,
                b: 0x9a,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0xd3,
                g: 0xd7,
                b: 0xcf,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x55,
                g: 0x57,
                b: 0x53,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0xef,
                g: 0x29,
                b: 0x29,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x8a,
                g: 0xe2,
                b: 0x34,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0xfc,
                g: 0xe9,
                b: 0x4f,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x72,
                g: 0x9f,
                b: 0xcf,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0xad,
                g: 0x7f,
                b: 0xa8,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0x34,
                g: 0xe2,
                b: 0xe2,
            }),
            rgb_to_hsla(TerminalRgb {
                r: 0xee,
                g: 0xee,
                b: 0xec,
            }),
        ];
        let mut extended_colors = [Hsla::default(); 256];
        extended_colors[0..16].copy_from_slice(&ansi_colors);
        let mut idx = 16;
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    extended_colors[idx] = rgb_to_hsla(TerminalRgb {
                        r: if r == 0 { 0 } else { 55 + r * 40 },
                        g: if g == 0 { 0 } else { 55 + g * 40 },
                        b: if b == 0 { 0 } else { 55 + b * 40 },
                    });
                    idx += 1;
                }
            }
        }
        for i in 0..24 {
            let gray = (8 + i * 10) as u8;
            extended_colors[232 + i] = rgb_to_hsla(TerminalRgb {
                r: gray,
                g: gray,
                b: gray,
            });
        }

        Self {
            ansi_colors,
            extended_colors,
            foreground: rgb_to_hsla(TerminalRgb {
                r: 0xd6,
                g: 0xda,
                b: 0xe2,
            }),
            background: rgb_to_hsla(TerminalRgb {
                r: 0x11,
                g: 0x14,
                b: 0x1a,
            }),
            cursor: rgb_to_hsla(TerminalRgb {
                r: 0xf3,
                g: 0xc9,
                b: 0x6b,
            }),
            selection: rgb_to_hsla(TerminalRgb {
                r: 0x26,
                g: 0x4f,
                b: 0x78,
            }),
        }
    }
}

impl ColorPalette {
    pub fn builder() -> ColorPaletteBuilder {
        ColorPaletteBuilder::new()
    }

    fn background(&self) -> Hsla {
        self.background
    }

    fn foreground(&self) -> Hsla {
        self.foreground
    }

    fn cursor(&self) -> Hsla {
        self.cursor
    }

    fn is_dark(&self) -> bool {
        relative_luminance(hsla_to_rgb(self.background))
            < relative_luminance(hsla_to_rgb(self.foreground))
    }

    pub(crate) fn foreground_osc_payload(&self) -> String {
        osc_color_payload(self.foreground)
    }

    pub(crate) fn background_osc_payload(&self) -> String {
        osc_color_payload(self.background)
    }

    fn resolve_fg(&self, color: &TerminalScreenColor, bold: bool, dim: bool) -> Hsla {
        let mut resolved = self.resolve_screen_color(color, self.foreground);
        if bold
            && let TerminalScreenColor::Indexed { index } = color
            && *index < 8
        {
            resolved = self.extended_colors[*index as usize + 8];
        }
        if dim {
            resolved = dim_color(resolved, self.background);
        }
        resolved
    }

    fn resolve_bg(&self, color: &TerminalScreenColor) -> Hsla {
        self.resolve_screen_color(color, self.background)
    }

    fn resolve_screen_color(&self, color: &TerminalScreenColor, default: Hsla) -> Hsla {
        match color {
            TerminalScreenColor::Default => default,
            TerminalScreenColor::Rgb { r, g, b } => rgb_to_hsla(TerminalRgb {
                r: *r,
                g: *g,
                b: *b,
            }),
            TerminalScreenColor::Indexed { index } => self
                .extended_colors
                .get(*index as usize)
                .copied()
                .unwrap_or(default),
            TerminalScreenColor::Named { name } => self.resolve_named(name, default),
        }
    }

    fn resolve_named(&self, name: &str, default: Hsla) -> Hsla {
        match name {
            "foreground" => self.foreground,
            "background" => self.background,
            "cursor" => self.cursor,
            "selection" => self.selection,
            "brightForeground" | "bright_foreground" => brighten_color(self.foreground),
            "dimForeground" | "dim_foreground" => dim_color(self.foreground, self.background),
            _ => default,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColorPaletteBuilder {
    palette: ColorPalette,
}

impl ColorPaletteBuilder {
    fn new() -> Self {
        Self {
            palette: ColorPalette::default(),
        }
    }

    pub fn background(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.background = rgb_to_hsla(TerminalRgb { r, g, b });
        self
    }

    pub fn foreground(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.foreground = rgb_to_hsla(TerminalRgb { r, g, b });
        self
    }

    pub fn cursor(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.cursor = rgb_to_hsla(TerminalRgb { r, g, b });
        self
    }

    pub fn selection(mut self, r: u8, g: u8, b: u8) -> Self {
        self.palette.selection = rgb_to_hsla(TerminalRgb { r, g, b });
        self
    }

    pub fn black(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(0, r, g, b)
    }
    pub fn red(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(1, r, g, b)
    }
    pub fn green(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(2, r, g, b)
    }
    pub fn yellow(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(3, r, g, b)
    }
    pub fn blue(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(4, r, g, b)
    }
    pub fn magenta(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(5, r, g, b)
    }
    pub fn cyan(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(6, r, g, b)
    }
    pub fn white(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(7, r, g, b)
    }
    pub fn bright_black(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(8, r, g, b)
    }
    pub fn bright_red(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(9, r, g, b)
    }
    pub fn bright_green(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(10, r, g, b)
    }
    pub fn bright_yellow(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(11, r, g, b)
    }
    pub fn bright_blue(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(12, r, g, b)
    }
    pub fn bright_magenta(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(13, r, g, b)
    }
    pub fn bright_cyan(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(14, r, g, b)
    }
    pub fn bright_white(self, r: u8, g: u8, b: u8) -> Self {
        self.ansi(15, r, g, b)
    }

    fn ansi(mut self, index: usize, r: u8, g: u8, b: u8) -> Self {
        let color = rgb_to_hsla(TerminalRgb { r, g, b });
        self.palette.ansi_colors[index] = color;
        self.palette.extended_colors[index] = color;
        self
    }

    pub fn build(self) -> ColorPalette {
        self.palette
    }
}

fn rgb_to_hsla(rgb: TerminalRgb) -> Hsla {
    gpui_rgb(rgb.r, rgb.g, rgb.b)
}

fn hsla_to_rgb(color: Hsla) -> TerminalRgb {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    TerminalRgb {
        r: channel(rgba.r),
        g: channel(rgba.g),
        b: channel(rgba.b),
    }
}

/// xterm dynamic-color payload ("rgb:rrrr/gggg/bbbb") for OSC 10/11 set/reply.
fn osc_color_payload(color: Hsla) -> String {
    let rgb = hsla_to_rgb(color);
    format!(
        "rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}",
        rgb.r, rgb.r, rgb.g, rgb.g, rgb.b, rgb.b
    )
}

fn relative_luminance(rgb: TerminalRgb) -> f32 {
    let channel = |value: u8| {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgb.r) + 0.7152 * channel(rgb.g) + 0.0722 * channel(rgb.b)
}

fn contrast_ratio(foreground: TerminalRgb, background: TerminalRgb) -> f32 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    let (lighter, darker) = if foreground >= background {
        (foreground, background)
    } else {
        (background, foreground)
    };
    (lighter + 0.05) / (darker + 0.05)
}

fn gpui_rgb(r: u8, g: u8, b: u8) -> Hsla {
    rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
}

fn mix_rgb(from: TerminalRgb, to: TerminalRgb, to_ratio: f32) -> TerminalRgb {
    let to_ratio = to_ratio.clamp(0.0, 1.0);
    let from_ratio = 1.0 - to_ratio;
    let mix = |from: u8, to: u8| -> u8 {
        (from as f32 * from_ratio + to as f32 * to_ratio)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    TerminalRgb {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
    }
}

fn dim_color(color: Hsla, background: Hsla) -> Hsla {
    // "Dim" means lower contrast against the background, not "darker". Blending
    // a fixed fraction toward the actual background fades the text on both dark
    // themes (toward black) and light themes (toward white), then clamps to a
    // readable floor so TUIs that mark bright/white text as faint do not vanish
    // on light terminal themes.
    const DIM_BLEND: f32 = 0.4;
    const MIN_DIM_CONTRAST: f32 = 3.0;
    let foreground = hsla_to_rgb(color);
    let background = hsla_to_rgb(background);
    let dimmed = mix_rgb(foreground, background, DIM_BLEND);
    if contrast_ratio(dimmed, background) >= MIN_DIM_CONTRAST {
        return rgb_to_hsla(dimmed);
    }

    let target = if relative_luminance(background) > 0.5 {
        TerminalRgb { r: 0, g: 0, b: 0 }
    } else {
        TerminalRgb {
            r: 255,
            g: 255,
            b: 255,
        }
    };
    for step in 1..=20 {
        let candidate = mix_rgb(dimmed, target, step as f32 / 20.0);
        if contrast_ratio(candidate, background) >= MIN_DIM_CONTRAST {
            return rgb_to_hsla(candidate);
        }
    }
    rgb_to_hsla(target)
}

fn brighten_color(mut color: Hsla) -> Hsla {
    color.l = (color.l * 1.2).min(1.0);
    color
}
