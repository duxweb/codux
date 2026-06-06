use super::*;

mod claude;
mod agy;
mod codewhale;
mod codex;
mod gemini;
mod kiro;
mod opencode;

pub(super) fn history_source_drivers() -> &'static [HistorySourceDriver] {
    &[
        claude::DRIVER,
        codex::DRIVER,
        gemini::DRIVER,
        agy::DRIVER,
        kiro::DRIVER,
        codewhale::DRIVER,
        opencode::DRIVER,
    ]
}

pub(super) fn history_source_progress(source: &str) -> f64 {
    match source {
        "claude" => 0.38,
        "codex" => 0.58,
        "gemini" => 0.74,
        "agy" => 0.78,
        "kiro" => 0.82,
        "codewhale" => 0.86,
        "opencode" => 0.88,
        _ => 0.88,
    }
}
