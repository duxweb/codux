use super::types::TrackedProject;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) fn upsert_project(
    projects: &mut HashMap<String, TrackedProject>,
    id: String,
    name: String,
    path: String,
) -> bool {
    if id.trim().is_empty() || path.trim().is_empty() {
        return false;
    }
    let mut inserted = false;
    projects
        .entry(id.clone())
        .and_modify(|project| {
            project.name = name.clone();
            project.path = path.clone();
        })
        .or_insert_with(|| {
            inserted = true;
            TrackedProject {
                id,
                name,
                path,
                last_git_refresh: None,
                last_remote_git_refresh: None,
                last_git_changed_refresh: None,
                last_ai_refresh: Some(Instant::now()),
            }
        });
    inserted
}

pub(super) fn projects_due_for_git_interval(
    projects: &Mutex<HashMap<String, TrackedProject>>,
    active_project_id: Option<&str>,
    is_foreground: bool,
    foreground_interval: Duration,
    background_interval: Duration,
    max_background: usize,
) -> Vec<TrackedProject> {
    let now = Instant::now();
    let Ok(mut guard) = projects.lock() else {
        return Vec::new();
    };
    let mut foreground_due = Vec::new();
    let mut background_due = Vec::new();

    for project in guard.values_mut() {
        let is_active_foreground = is_foreground && active_project_id == Some(project.id.as_str());
        let interval = if is_active_foreground {
            foreground_interval
        } else {
            background_interval
        };
        let is_due = project
            .last_git_refresh
            .map(|value| now.duration_since(value) >= interval)
            .unwrap_or(true);
        if !is_due {
            continue;
        }
        if is_active_foreground {
            project.last_git_refresh = Some(now);
            foreground_due.push(project.clone());
        } else if background_due.len() < max_background {
            project.last_git_refresh = Some(now);
            background_due.push(project.clone());
        }
    }

    foreground_due.extend(background_due);
    foreground_due
}

pub(super) fn configured_interval_seconds(value: &str, minimum: u64) -> Option<Duration> {
    let seconds = value.trim().parse::<u64>().ok()?;
    (seconds > 0).then(|| Duration::from_secs(seconds.max(minimum)))
}
