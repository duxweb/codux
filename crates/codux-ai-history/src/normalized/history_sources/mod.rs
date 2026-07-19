use super::*;

mod agy;
mod claude;
mod codewhale;
mod codex;
mod kimi;
mod kiro;
mod mimo;
mod omp;
mod opencode;

pub fn history_source_drivers() -> &'static [HistorySourceDriver] {
    &[
        claude::DRIVER,
        codex::DRIVER,
        agy::DRIVER,
        kiro::DRIVER,
        codewhale::DRIVER,
        kimi::DRIVER,
        mimo::DRIVER,
        omp::DRIVER,
        opencode::DRIVER,
    ]
}

pub fn history_source_progress(source: &str) -> f64 {
    match source {
        "claude" => 0.38,
        "codex" => 0.58,
        "agy" => 0.78,
        "kiro" => 0.82,
        "codewhale" => 0.86,
        "kimi" => 0.87,
        "mimo" => 0.875,
        "omp" => 0.88,
        "opencode" => 0.89,
        _ => 0.88,
    }
}
