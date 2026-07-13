#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TerminalWindowSize {
    num_lines: u16,
    num_cols: u16,
    cell_width: u16,
    cell_height: u16,
}

#[derive(Clone)]
enum TerminalUiEvent {
    Error(String),
    Reconnected,
    Viewport {
        remote_owner: bool,
        generation: u64,
        cols: u16,
        rows: u16,
    },
    Exit,
}
