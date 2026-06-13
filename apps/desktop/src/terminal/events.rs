#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TerminalWindowSize {
    num_lines: u16,
    num_cols: u16,
    cell_width: u16,
    cell_height: u16,
}

#[derive(Clone)]
enum TerminalUiEvent {
    Wakeup,
    Error(String),
    Viewport {
        cols: u16,
        rows: u16,
        remote_owner: bool,
    },
    Exit,
}
