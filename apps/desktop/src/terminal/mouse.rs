#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseReportKind {
    Press,
    Release,
    Move,
    Wheel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalMouseButton {
    Left = 0,
    Middle = 1,
    Right = 2,
    LeftMove = 32,
    MiddleMove = 33,
    RightMove = 34,
    NoneMove = 35,
    ScrollUp = 64,
    ScrollDown = 65,
}

fn mouse_report_sequence(
    button: Option<MouseButton>,
    point: TerminalCellPoint,
    kind: MouseReportKind,
    modifiers: Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }

    let (button, pressed) = match kind {
        MouseReportKind::Press => (mouse_button(button?)?, true),
        MouseReportKind::Release => (mouse_button(button?)?, false),
        MouseReportKind::Move => {
            if !mode.intersects(TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG) {
                return None;
            }
            let button = mouse_move_button(button)?;
            if mode.contains(TermMode::MOUSE_DRAG)
                && matches!(button, TerminalMouseButton::NoneMove)
            {
                return None;
            }
            (button, true)
        }
        MouseReportKind::Wheel => (mouse_wheel_button(button?)?, true),
    };

    let mut code = button as u8;
    if modifiers.shift {
        code += 4;
    }
    if modifiers.alt {
        code += 8;
    }
    if modifiers.control {
        code += 16;
    }

    if mode.contains(TermMode::SGR_MOUSE) {
        let suffix = if pressed { 'M' } else { 'm' };
        return Some(
            format!(
                "\x1b[<{};{};{}{}",
                code,
                point.col + 1,
                point.row + 1,
                suffix
            )
            .into_bytes(),
        );
    }

    normal_mouse_report(
        point,
        if pressed {
            code
        } else {
            3 + (code - button as u8)
        },
        mode,
    )
}

fn mouse_button(button: MouseButton) -> Option<TerminalMouseButton> {
    match button {
        MouseButton::Left => Some(TerminalMouseButton::Left),
        MouseButton::Middle => Some(TerminalMouseButton::Middle),
        MouseButton::Right => Some(TerminalMouseButton::Right),
        MouseButton::Navigate(_) => None,
    }
}

fn mouse_move_button(button: Option<MouseButton>) -> Option<TerminalMouseButton> {
    match button {
        Some(MouseButton::Left) => Some(TerminalMouseButton::LeftMove),
        Some(MouseButton::Middle) => Some(TerminalMouseButton::MiddleMove),
        Some(MouseButton::Right) => Some(TerminalMouseButton::RightMove),
        Some(MouseButton::Navigate(_)) => None,
        None => Some(TerminalMouseButton::NoneMove),
    }
}

fn mouse_wheel_button(button: MouseButton) -> Option<TerminalMouseButton> {
    match button {
        MouseButton::Navigate(NavigationDirection::Back) => Some(TerminalMouseButton::ScrollUp),
        MouseButton::Navigate(NavigationDirection::Forward) => {
            Some(TerminalMouseButton::ScrollDown)
        }
        _ => None,
    }
}

fn ansi_named_color(index: usize) -> NamedColor {
    match index {
        0 => NamedColor::Black,
        1 => NamedColor::Red,
        2 => NamedColor::Green,
        3 => NamedColor::Yellow,
        4 => NamedColor::Blue,
        5 => NamedColor::Magenta,
        6 => NamedColor::Cyan,
        7 => NamedColor::White,
        8 => NamedColor::BrightBlack,
        9 => NamedColor::BrightRed,
        10 => NamedColor::BrightGreen,
        11 => NamedColor::BrightYellow,
        12 => NamedColor::BrightBlue,
        13 => NamedColor::BrightMagenta,
        14 => NamedColor::BrightCyan,
        15 => NamedColor::BrightWhite,
        _ => NamedColor::White,
    }
}

fn normal_mouse_report(
    point: TerminalCellPoint,
    button_code: u8,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    let max_point = if utf8 { 2015 } else { 223 };
    if point.row >= max_point || point.col >= max_point {
        return None;
    }

    let mut message = vec![b'\x1b', b'[', b'M', 32 + button_code];
    append_mouse_position(&mut message, point.col, utf8);
    append_mouse_position(&mut message, point.row, utf8);
    Some(message)
}

fn append_mouse_position(message: &mut Vec<u8>, position: usize, utf8: bool) {
    let encoded = 32 + 1 + position;
    if utf8 && position >= 95 {
        message.push((0xC0 + encoded / 64) as u8);
        message.push((0x80 + (encoded & 63)) as u8);
    } else {
        message.push(encoded as u8);
    }
}
