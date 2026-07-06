#[derive(Clone, Copy)]
enum TerminalCellGraphic {
    Block(TerminalBlockGraphic),
    Box(TerminalBoxGraphic),
    Powerline(TerminalPowerlineGraphic),
}

// Powerline separators (U+E0B0–U+E0BF) are drawn as cell-exact vectors: font
// glyphs follow the em box and leave gaps against the padded terminal cell.
#[derive(Clone, Copy)]
enum TerminalPowerlineGraphic {
    TriangleRight,
    ChevronRight,
    TriangleLeft,
    ChevronLeft,
    SemicircleRight,
    SemicircleRightLine,
    SemicircleLeft,
    SemicircleLeftLine,
    TriangleLowerLeft,
    DiagonalBack,
    TriangleLowerRight,
    DiagonalForward,
    TriangleUpperLeft,
    TriangleUpperRight,
}

#[derive(Clone, Copy)]
enum TerminalBlockGraphic {
    Full,
    Upper(f32),
    Lower(f32),
    Left(f32),
    Right(f32),
    Quadrants {
        upper_left: bool,
        upper_right: bool,
        lower_left: bool,
        lower_right: bool,
    },
}

#[derive(Clone, Copy)]
struct TerminalBoxGraphic {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    weight: TerminalBoxWeight,
    double: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TerminalBoxWeight {
    Light,
    Heavy,
}

fn terminal_cell_codepoint(text: &str) -> Option<u32> {
    let mut chars = text.chars();
    let codepoint = chars.next()? as u32;
    chars.next().is_none().then_some(codepoint)
}

fn terminal_builtin_graphic(codepoint: u32) -> Option<TerminalCellGraphic> {
    if (0xE0B0..=0xE0BF).contains(&codepoint) {
        return terminal_powerline_graphic(codepoint).map(TerminalCellGraphic::Powerline);
    }
    if !(0x2500..=0x259F).contains(&codepoint) {
        return None;
    }
    terminal_block_graphic(codepoint)
        .map(TerminalCellGraphic::Block)
        .or_else(|| terminal_box_graphic(codepoint).map(TerminalCellGraphic::Box))
}

fn terminal_powerline_graphic(codepoint: u32) -> Option<TerminalPowerlineGraphic> {
    match codepoint {
        0xE0B0 => Some(TerminalPowerlineGraphic::TriangleRight),
        0xE0B1 => Some(TerminalPowerlineGraphic::ChevronRight),
        0xE0B2 => Some(TerminalPowerlineGraphic::TriangleLeft),
        0xE0B3 => Some(TerminalPowerlineGraphic::ChevronLeft),
        0xE0B4 => Some(TerminalPowerlineGraphic::SemicircleRight),
        0xE0B5 => Some(TerminalPowerlineGraphic::SemicircleRightLine),
        0xE0B6 => Some(TerminalPowerlineGraphic::SemicircleLeft),
        0xE0B7 => Some(TerminalPowerlineGraphic::SemicircleLeftLine),
        0xE0B8 => Some(TerminalPowerlineGraphic::TriangleLowerLeft),
        0xE0B9 | 0xE0BF => Some(TerminalPowerlineGraphic::DiagonalBack),
        0xE0BA => Some(TerminalPowerlineGraphic::TriangleLowerRight),
        0xE0BB | 0xE0BD => Some(TerminalPowerlineGraphic::DiagonalForward),
        0xE0BC => Some(TerminalPowerlineGraphic::TriangleUpperLeft),
        0xE0BE => Some(TerminalPowerlineGraphic::TriangleUpperRight),
        _ => None,
    }
}

fn terminal_block_graphic(codepoint: u32) -> Option<TerminalBlockGraphic> {
    match codepoint {
        0x2580 => Some(TerminalBlockGraphic::Upper(0.5)),
        0x2581 => Some(TerminalBlockGraphic::Lower(0.125)),
        0x2582 => Some(TerminalBlockGraphic::Lower(0.25)),
        0x2583 => Some(TerminalBlockGraphic::Lower(0.375)),
        0x2584 => Some(TerminalBlockGraphic::Lower(0.5)),
        0x2585 => Some(TerminalBlockGraphic::Lower(0.625)),
        0x2586 => Some(TerminalBlockGraphic::Lower(0.75)),
        0x2587 => Some(TerminalBlockGraphic::Lower(0.875)),
        0x2588 => Some(TerminalBlockGraphic::Full),
        0x2589 => Some(TerminalBlockGraphic::Left(0.875)),
        0x258A => Some(TerminalBlockGraphic::Left(0.75)),
        0x258B => Some(TerminalBlockGraphic::Left(0.625)),
        0x258C => Some(TerminalBlockGraphic::Left(0.5)),
        0x258D => Some(TerminalBlockGraphic::Left(0.375)),
        0x258E => Some(TerminalBlockGraphic::Left(0.25)),
        0x258F => Some(TerminalBlockGraphic::Left(0.125)),
        0x2590 => Some(TerminalBlockGraphic::Right(0.5)),
        0x2594 => Some(TerminalBlockGraphic::Upper(0.125)),
        0x2595 => Some(TerminalBlockGraphic::Right(0.125)),
        0x2596 => Some(TerminalBlockGraphic::Quadrants {
            upper_left: false,
            upper_right: false,
            lower_left: true,
            lower_right: false,
        }),
        0x2597 => Some(TerminalBlockGraphic::Quadrants {
            upper_left: false,
            upper_right: false,
            lower_left: false,
            lower_right: true,
        }),
        0x2598 => Some(TerminalBlockGraphic::Quadrants {
            upper_left: true,
            upper_right: false,
            lower_left: false,
            lower_right: false,
        }),
        0x2599 => Some(TerminalBlockGraphic::Quadrants {
            upper_left: true,
            upper_right: false,
            lower_left: true,
            lower_right: true,
        }),
        0x259A => Some(TerminalBlockGraphic::Quadrants {
            upper_left: true,
            upper_right: false,
            lower_left: false,
            lower_right: true,
        }),
        0x259B => Some(TerminalBlockGraphic::Quadrants {
            upper_left: true,
            upper_right: true,
            lower_left: true,
            lower_right: false,
        }),
        0x259C => Some(TerminalBlockGraphic::Quadrants {
            upper_left: true,
            upper_right: true,
            lower_left: false,
            lower_right: true,
        }),
        0x259D => Some(TerminalBlockGraphic::Quadrants {
            upper_left: false,
            upper_right: true,
            lower_left: false,
            lower_right: false,
        }),
        0x259E => Some(TerminalBlockGraphic::Quadrants {
            upper_left: false,
            upper_right: true,
            lower_left: true,
            lower_right: false,
        }),
        0x259F => Some(TerminalBlockGraphic::Quadrants {
            upper_left: false,
            upper_right: true,
            lower_left: true,
            lower_right: true,
        }),
        _ => None,
    }
}

fn terminal_box_graphic(codepoint: u32) -> Option<TerminalBoxGraphic> {
    let graphic = match codepoint {
        0x2500 => terminal_box(true, true, false, false, TerminalBoxWeight::Light, false),
        0x2501 => terminal_box(true, true, false, false, TerminalBoxWeight::Heavy, false),
        0x2502 => terminal_box(false, false, true, true, TerminalBoxWeight::Light, false),
        0x2503 => terminal_box(false, false, true, true, TerminalBoxWeight::Heavy, false),
        0x2504 | 0x2505 | 0x2508 | 0x2509 => {
            terminal_box(true, true, false, false, TerminalBoxWeight::Light, false)
        }
        0x2506 | 0x2507 | 0x250A | 0x250B => {
            terminal_box(false, false, true, true, TerminalBoxWeight::Light, false)
        }
        0x250C => terminal_box(false, true, false, true, TerminalBoxWeight::Light, false),
        0x250D..=0x250F => terminal_box(false, true, false, true, TerminalBoxWeight::Heavy, false),
        0x2510 => terminal_box(true, false, false, true, TerminalBoxWeight::Light, false),
        0x2511..=0x2513 => terminal_box(true, false, false, true, TerminalBoxWeight::Heavy, false),
        0x2514 => terminal_box(false, true, true, false, TerminalBoxWeight::Light, false),
        0x2515..=0x2517 => terminal_box(false, true, true, false, TerminalBoxWeight::Heavy, false),
        0x2518 => terminal_box(true, false, true, false, TerminalBoxWeight::Light, false),
        0x2519..=0x251B => terminal_box(true, false, true, false, TerminalBoxWeight::Heavy, false),
        0x251C => terminal_box(false, true, true, true, TerminalBoxWeight::Light, false),
        0x251D..=0x2523 => terminal_box(false, true, true, true, TerminalBoxWeight::Heavy, false),
        0x2524 => terminal_box(true, false, true, true, TerminalBoxWeight::Light, false),
        0x2525..=0x252B => terminal_box(true, false, true, true, TerminalBoxWeight::Heavy, false),
        0x252C => terminal_box(true, true, false, true, TerminalBoxWeight::Light, false),
        0x252D..=0x2533 => terminal_box(true, true, false, true, TerminalBoxWeight::Heavy, false),
        0x2534 => terminal_box(true, true, true, false, TerminalBoxWeight::Light, false),
        0x2535..=0x253B => terminal_box(true, true, true, false, TerminalBoxWeight::Heavy, false),
        0x253C => terminal_box(true, true, true, true, TerminalBoxWeight::Light, false),
        0x253D..=0x254B => terminal_box(true, true, true, true, TerminalBoxWeight::Heavy, false),
        0x2550 => terminal_box(true, true, false, false, TerminalBoxWeight::Light, true),
        0x2551 => terminal_box(false, false, true, true, TerminalBoxWeight::Light, true),
        0x2554 => terminal_box(false, true, false, true, TerminalBoxWeight::Light, true),
        0x2557 => terminal_box(true, false, false, true, TerminalBoxWeight::Light, true),
        0x255A => terminal_box(false, true, true, false, TerminalBoxWeight::Light, true),
        0x255D => terminal_box(true, false, true, false, TerminalBoxWeight::Light, true),
        0x2560 => terminal_box(false, true, true, true, TerminalBoxWeight::Light, true),
        0x2563 => terminal_box(true, false, true, true, TerminalBoxWeight::Light, true),
        0x2566 => terminal_box(true, true, false, true, TerminalBoxWeight::Light, true),
        0x2569 => terminal_box(true, true, true, false, TerminalBoxWeight::Light, true),
        0x256C => terminal_box(true, true, true, true, TerminalBoxWeight::Light, true),
        0x2574 => terminal_box(true, false, false, false, TerminalBoxWeight::Light, false),
        0x2575 => terminal_box(false, false, true, false, TerminalBoxWeight::Light, false),
        0x2576 => terminal_box(false, true, false, false, TerminalBoxWeight::Light, false),
        0x2577 => terminal_box(false, false, false, true, TerminalBoxWeight::Light, false),
        0x2578 => terminal_box(true, false, false, false, TerminalBoxWeight::Heavy, false),
        0x2579 => terminal_box(false, false, true, false, TerminalBoxWeight::Heavy, false),
        0x257A => terminal_box(false, true, false, false, TerminalBoxWeight::Heavy, false),
        0x257B => terminal_box(false, false, false, true, TerminalBoxWeight::Heavy, false),
        _ => return None,
    };
    Some(graphic)
}

fn terminal_box(
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    weight: TerminalBoxWeight,
    double: bool,
) -> TerminalBoxGraphic {
    TerminalBoxGraphic {
        left,
        right,
        up,
        down,
        weight,
        double,
    }
}
