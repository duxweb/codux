use crate::ai_runtime::{
    constants::CLAUDE_STALE_PRELAUNCH_OPEN_TURN_SOURCE,
    probe::{
        common::{
            first_object_deep, first_string_deep, is_awaiting_user_decision, json_i64, now_seconds,
            parse_iso8601_seconds,
        },
        paths::{
            claude_project_log_paths, file_modified_after_start, file_modified_millis,
            paths_equivalent,
        },
        preview::sanitized_preview_from_values,
    },
    snapshot::{AIPlanItem, AIPlanSnapshot, AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    state::normalized_string,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{BufRead, BufReader, Seek},
    path::{Path, PathBuf},
};

pub(crate) fn probe_claude_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    probe_claude_runtime_inner(request, None)
}

#[derive(Default)]
pub(crate) struct ClaudeProbeCache {
    files: HashMap<ClaudeProbeCacheKey, ClaudeProbeCacheEntry>,
}

impl ClaudeProbeCache {
    pub(crate) fn retain_terminals(&mut self, terminal_ids: &std::collections::HashSet<String>) {
        self.files
            .retain(|_, entry| terminal_ids.contains(&entry.terminal_id));
    }
}

#[derive(Hash, PartialEq, Eq)]
struct ClaudeProbeCacheKey {
    path: PathBuf,
    project_path: String,
    external_session_id: String,
}

struct ClaudeProbeCacheEntry {
    terminal_id: String,
    size: u64,
    modified_nanos: u128,
    parsed_offset: u64,
    has_pending_partial: bool,
    matched: bool,
    aggregate: ClaudeAggregate,
}

pub(crate) fn probe_claude_runtime_with_cache(
    request: &AIRuntimeProbeRequest,
    cache: &mut ClaudeProbeCache,
) -> Option<AIRuntimeContextSnapshot> {
    probe_claude_runtime_inner(request, Some(cache))
}

fn probe_claude_runtime_inner(
    request: &AIRuntimeProbeRequest,
    cache: Option<&mut ClaudeProbeCache>,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let file_urls = claude_project_log_paths(&project_path);
    probe_claude_runtime_from_paths(request, &project_path, &file_urls, cache)
}

fn probe_claude_runtime_from_paths(
    request: &AIRuntimeProbeRequest,
    project_path: &str,
    file_urls: &[PathBuf],
    mut cache: Option<&mut ClaudeProbeCache>,
) -> Option<AIRuntimeContextSnapshot> {
    let candidate = select_claude_session_candidate(file_urls, project_path, request)?;
    let external_id = candidate.external_session_id;
    let selected_paths = claude_log_paths_for_session(file_urls, &external_id);
    let mut aggregate: Option<ClaudeAggregate> = None;
    // Track the matched file with the most recent activity so the supervisor can
    // size+mtime watch it (parity with codex); claude sessions are otherwise
    // only re-probed on the 5s interval.
    let mut transcript_path: Option<String> = None;
    let mut transcript_seen_at = f64::MIN;
    for file_url in selected_paths {
        let next = if let Some(cache) = cache.as_deref_mut() {
            parse_claude_log_runtime_state_cached(
                &file_url,
                project_path,
                &external_id,
                &request.terminal_id,
                cache,
            )
        } else {
            parse_claude_log_runtime_state(&file_url, project_path, &external_id)
        };
        let Some(next) = next else {
            continue;
        };
        if next.updated_at >= transcript_seen_at {
            transcript_seen_at = next.updated_at;
            transcript_path = Some(file_url.display().to_string());
        }
        aggregate = Some(match aggregate {
            Some(existing) => existing.merge(next),
            None => next,
        });
    }
    let aggregate = aggregate?;
    let plan = aggregate.plan(&external_id);
    let started_at = aggregate.started_at();
    let stale_prelaunch_open_turn =
        stale_prelaunch_open_turn(&aggregate, transcript_path.as_deref(), request.started_at);
    let stale_completed_at = stale_prelaunch_open_turn.then(|| {
        request
            .updated_at
            .max(request.started_at.unwrap_or(request.updated_at))
    });
    let completed_at = stale_completed_at.or_else(|| aggregate.completed_at());
    let mut response_state = aggregate.response_state();
    if stale_prelaunch_open_turn {
        response_state = Some("idle".to_string());
    } else if aggregate.needs_user_input(now_seconds()) {
        response_state = Some("needsInput".to_string());
    }
    let was_interrupted = !stale_prelaunch_open_turn && aggregate.was_interrupted();
    let has_completed_turn = !stale_prelaunch_open_turn && aggregate.has_completed_turn();
    Some(AIRuntimeContextSnapshot {
        tool: "claude".to_string(),
        external_session_id: Some(external_id),
        transcript_path,
        model: aggregate.model,
        assistant_preview: aggregate.assistant_preview,
        input_tokens: aggregate.input_tokens,
        output_tokens: aggregate.output_tokens,
        cached_input_tokens: aggregate.cached_input_tokens,
        total_tokens: aggregate.total_tokens,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        updated_at: aggregate
            .updated_at
            .max(stale_completed_at.unwrap_or(request.updated_at))
            .max(request.updated_at),
        started_at,
        completed_at,
        response_state,
        was_interrupted,
        has_completed_turn,
        session_origin: "unknown".to_string(),
        source: if stale_prelaunch_open_turn {
            CLAUDE_STALE_PRELAUNCH_OPEN_TURN_SOURCE
        } else {
            "probe"
        }
        .to_string(),
        plan,
    })
}

fn stale_prelaunch_open_turn(
    aggregate: &ClaudeAggregate,
    transcript_path: Option<&str>,
    launch_started_at: Option<f64>,
) -> bool {
    let Some(launch_started_at) = launch_started_at else {
        return false;
    };
    if aggregate.response_state().as_deref() != Some("responding") {
        return false;
    }
    let Some(last_event_at) = aggregate
        .last_event_at
        .or_else(|| transcript_path.and_then(|path| transcript_modified_seconds(Path::new(path))))
    else {
        return false;
    };
    last_event_at + 1.0 < launch_started_at
}

fn transcript_modified_seconds(file_path: &Path) -> Option<f64> {
    fs::metadata(file_path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs_f64())
}

#[derive(Clone)]
struct ClaudeSessionCandidate {
    external_session_id: String,
    modified_millis: u128,
}

fn select_claude_session_candidate(
    paths: &[PathBuf],
    project_path: &str,
    request: &AIRuntimeProbeRequest,
) -> Option<ClaudeSessionCandidate> {
    let bound = normalized_string(request.external_session_id.as_deref()).and_then(|id| {
        claude_exact_session_path(paths, &id).map(|path| ClaudeSessionCandidate {
            external_session_id: id,
            modified_millis: file_modified_millis(&path).unwrap_or(0),
        })
    });
    let min_modified_millis = bound.as_ref().map(|candidate| candidate.modified_millis);
    let fallback = freshest_claude_session_candidate_since(
        paths,
        project_path,
        request.started_at,
        &request.occupied_external_session_ids,
        min_modified_millis,
    );
    match (bound, fallback) {
        (Some(bound), Some(fallback)) if fallback.modified_millis > bound.modified_millis => {
            Some(fallback)
        }
        (Some(bound), _) => Some(bound),
        (None, fallback) => fallback,
    }
}

fn claude_exact_session_path(paths: &[PathBuf], external_session_id: &str) -> Option<PathBuf> {
    paths
        .iter()
        .filter(|path| {
            path.file_stem().and_then(|value| value.to_str()) == Some(external_session_id)
        })
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
        .cloned()
}

fn freshest_claude_session_candidate_since(
    paths: &[PathBuf],
    project_path: &str,
    started_at: Option<f64>,
    occupied_external_session_ids: &std::collections::HashSet<String>,
    min_modified_millis: Option<u128>,
) -> Option<ClaudeSessionCandidate> {
    let mut paths = paths
        .iter()
        .map(|path| (file_modified_millis(path).unwrap_or(0), path.clone()))
        .filter(|(modified_millis, path)| {
            min_modified_millis
                .map(|min_modified_millis| *modified_millis > min_modified_millis)
                .unwrap_or(true)
                && file_modified_after_start(path, started_at)
        })
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| right.0.cmp(&left.0));
    for (modified_millis, path) in paths {
        let Some(external_session_id) = claude_session_id_for_project_since(
            &path,
            project_path,
            started_at,
            occupied_external_session_ids,
        ) else {
            continue;
        };
        return Some(ClaudeSessionCandidate {
            external_session_id,
            modified_millis,
        });
    }
    None
}

fn claude_session_id_for_project_since(
    path: &Path,
    project_path: &str,
    started_at: Option<f64>,
    occupied_external_session_ids: &std::collections::HashSet<String>,
) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut found = None;
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(bytes) = reader.read_line(&mut line) else {
            break;
        };
        if bytes == 0 {
            break;
        }
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(started_at) = started_at {
            let Some(timestamp) = row
                .get("timestamp")
                .and_then(|value| value.as_str())
                .and_then(parse_iso8601_seconds)
            else {
                continue;
            };
            if timestamp + 1.0 < started_at {
                continue;
            }
        }
        if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str())
            && !paths_equivalent(Some(cwd), project_path)
        {
            continue;
        }
        let Some(id) = row
            .get("sessionId")
            .and_then(|value| value.as_str())
            .and_then(|value| normalized_string(Some(value)))
        else {
            continue;
        };
        if occupied_external_session_ids.contains(&id) {
            continue;
        }
        found = Some(id);
    }
    found
}

fn claude_log_paths_for_session(paths: &[PathBuf], external_session_id: &str) -> Vec<PathBuf> {
    let exact = paths
        .iter()
        .filter(|path| {
            path.file_stem().and_then(|value| value.to_str()) == Some(external_session_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    if exact.is_empty() {
        paths.to_vec()
    } else {
        exact
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ClaudeAggregate {
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    updated_at: f64,
    last_event_at: Option<f64>,
    last_user_at: f64,
    last_completion_at: f64,
    last_interrupted_at: f64,
    last_completed_turn_at: f64,
    /// Most recent assistant message carrying a `tool_use` block, and the most
    /// recent user message carrying its `tool_result`. While a tool is blocked on
    /// a permission prompt the `tool_use` is written but no `tool_result`
    /// follows, so `last_tool_use_at > last_tool_result_at` is the pending-call
    /// signature behind `needsInput` detection.
    last_tool_use_at: f64,
    last_tool_result_at: f64,
    /// Last `permission-mode` entry's mode (e.g. `bypassPermissions`, `default`,
    /// `acceptEdits`, `plan`). `bypassPermissions` means no prompt can ever fire,
    /// so a pending call there is the CLI working, not waiting.
    permission_mode: Option<String>,
    tasks: BTreeMap<String, AIPlanItem>,
    task_list: Option<Vec<AIPlanItem>>,
    task_updated_at: f64,
}

impl ClaudeAggregate {
    fn merge(self, other: Self) -> Self {
        Self {
            model: other.model.or(self.model),
            assistant_preview: other.assistant_preview.or(self.assistant_preview),
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cached_input_tokens: self.cached_input_tokens + other.cached_input_tokens,
            total_tokens: self.total_tokens + other.total_tokens,
            updated_at: self.updated_at.max(other.updated_at),
            last_event_at: self.last_event_at.or(other.last_event_at).map(|left| {
                other
                    .last_event_at
                    .map(|right| left.max(right))
                    .unwrap_or(left)
            }),
            last_user_at: self.last_user_at.max(other.last_user_at),
            last_completion_at: self.last_completion_at.max(other.last_completion_at),
            last_interrupted_at: self.last_interrupted_at.max(other.last_interrupted_at),
            last_completed_turn_at: self
                .last_completed_turn_at
                .max(other.last_completed_turn_at),
            last_tool_use_at: self.last_tool_use_at.max(other.last_tool_use_at),
            last_tool_result_at: self.last_tool_result_at.max(other.last_tool_result_at),
            permission_mode: other.permission_mode.or(self.permission_mode),
            tasks: merge_claude_tasks(self.tasks, other.tasks),
            task_list: other.task_list.or(self.task_list),
            task_updated_at: self.task_updated_at.max(other.task_updated_at),
        }
    }

    fn plan(&self, session_id: &str) -> Option<AIPlanSnapshot> {
        let items = self
            .task_list
            .clone()
            .unwrap_or_else(|| self.tasks.values().cloned().collect());
        (!items.is_empty()).then_some(AIPlanSnapshot {
            source: "claude".to_string(),
            session_id: session_id.to_string(),
            updated_at: self.task_updated_at.max(self.updated_at),
            items,
        })
    }

    fn started_at(&self) -> Option<f64> {
        (self.last_user_at > 0.0).then_some(self.last_user_at)
    }

    fn completed_at(&self) -> Option<f64> {
        let completion = self.last_completed_turn_at.max(self.last_interrupted_at);
        (completion > 0.0).then_some(completion)
    }

    fn response_state(&self) -> Option<String> {
        if self.last_user_at <= 0.0 {
            return None;
        }
        if self.last_user_at > self.last_completion_at {
            Some("responding".to_string())
        } else {
            Some("idle".to_string())
        }
    }

    fn was_interrupted(&self) -> bool {
        if self.last_interrupted_at <= 0.0 {
            return false;
        }
        let latest_conflicting_at = self.last_user_at.max(self.last_completed_turn_at);
        self.last_interrupted_at >= latest_conflicting_at
    }

    fn has_completed_turn(&self) -> bool {
        if self.last_completed_turn_at <= 0.0 {
            return false;
        }
        let latest_conflicting_at = self.last_user_at.max(self.last_interrupted_at);
        self.last_completed_turn_at >= latest_conflicting_at
    }

    /// Whether the session's permission mode can still raise an approval prompt.
    /// Only `bypassPermissions` (codux's `--dangerously-skip-permissions`) silences
    /// every prompt; `default`/`acceptEdits`/`plan` all still gate some action.
    /// Unknown/absent (older CLIs) defaults to `true` so a wait is never silently
    /// dropped.
    fn prompts_possible(&self) -> bool {
        !matches!(self.permission_mode.as_deref(), Some("bypassPermissions"))
    }

    /// A `tool_use` is written with no matching `tool_result` yet -- the call is
    /// in flight. Combined with an idle gap (in the caller) this is the
    /// permission/elicitation wait signature.
    fn pending_tool_call(&self) -> bool {
        self.last_tool_use_at > 0.0 && self.last_tool_use_at > self.last_tool_result_at
    }

    /// The session is mid-turn with a tool call that has been written but left
    /// unanswered past the idle gap, in a mode that can still prompt -- i.e. the
    /// CLI is blocked waiting on the user. Idle is measured from the tool-use
    /// row's own timestamp, not `updated_at`, because timestamp-less metadata
    /// rows (permission-mode/mode/ai-title) pin `updated_at` to `now` on every
    /// read.
    fn needs_user_input(&self, now: f64) -> bool {
        is_awaiting_user_decision(
            self.response_state().as_deref() == Some("responding"),
            self.prompts_possible(),
            self.pending_tool_call(),
            self.last_tool_use_at,
            now,
        )
    }
}

fn claude_message_has_block(message: &Value, block_type: &str) -> bool {
    message
        .get("content")
        .and_then(|content| content.as_array())
        .map(|items| {
            items
                .iter()
                .any(|item| item.get("type").and_then(|value| value.as_str()) == Some(block_type))
        })
        .unwrap_or(false)
}

fn parse_claude_log_runtime_state(
    file_path: &Path,
    project_path: &str,
    external_session_id: &str,
) -> Option<ClaudeAggregate> {
    let file = fs::File::open(file_path).ok()?;
    let mut reader = BufReader::new(file);
    let mut aggregate = ClaudeAggregate::default();
    let matched = parse_claude_log_reader(
        &mut reader,
        project_path,
        external_session_id,
        &mut aggregate,
    );

    if !matched {
        return None;
    }
    Some(aggregate)
}

fn parse_claude_log_runtime_state_cached(
    file_path: &Path,
    project_path: &str,
    external_session_id: &str,
    terminal_id: &str,
    cache: &mut ClaudeProbeCache,
) -> Option<ClaudeAggregate> {
    let metadata = fs::metadata(file_path).ok()?;
    let size = metadata.len();
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let key = ClaudeProbeCacheKey {
        path: canonical_probe_path(file_path),
        project_path: project_path.to_string(),
        external_session_id: external_session_id.to_string(),
    };
    if let Some(entry) = cache.files.get(&key)
        && !entry.has_pending_partial
        && entry.size == size
        && entry.modified_nanos == modified_nanos
    {
        return entry.matched.then(|| entry.aggregate.clone());
    }

    let can_append = cache
        .files
        .get(&key)
        .map(|entry| {
            size >= entry.parsed_offset
                && (size > entry.size || (size == entry.size && entry.has_pending_partial))
                && modified_nanos >= entry.modified_nanos
        })
        .unwrap_or(false);
    let mut entry = if can_append {
        cache.files.remove(&key).unwrap()
    } else {
        ClaudeProbeCacheEntry {
            terminal_id: terminal_id.to_string(),
            size: 0,
            modified_nanos: 0,
            parsed_offset: 0,
            has_pending_partial: false,
            matched: false,
            aggregate: ClaudeAggregate::default(),
        }
    };

    let mut file = fs::File::open(file_path).ok()?;
    let start = if can_append { entry.parsed_offset } else { 0 };
    if file.seek(std::io::SeekFrom::Start(start)).is_err() {
        return None;
    }
    let mut reader = BufReader::new(file);
    let mut current_offset = start;
    let progress = parse_claude_log_reader_with_offset(
        &mut reader,
        project_path,
        external_session_id,
        &mut entry.aggregate,
        &mut current_offset,
        false,
    );
    entry.terminal_id = terminal_id.to_string();
    entry.size = size;
    entry.modified_nanos = modified_nanos;
    entry.parsed_offset = current_offset;
    entry.has_pending_partial = progress.has_pending_partial;
    entry.matched = entry.matched || progress.matched;
    let result = entry.matched.then(|| entry.aggregate.clone());
    cache.files.insert(key, entry);
    result
}

fn canonical_probe_path(file_path: &Path) -> PathBuf {
    file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf())
}

fn parse_claude_log_reader<R>(
    reader: &mut R,
    project_path: &str,
    external_session_id: &str,
    aggregate: &mut ClaudeAggregate,
) -> bool
where
    R: BufRead,
{
    let mut offset = 0;
    parse_claude_log_reader_with_offset(
        reader,
        project_path,
        external_session_id,
        aggregate,
        &mut offset,
        true,
    )
    .matched
}

struct ClaudeParseProgress {
    matched: bool,
    has_pending_partial: bool,
}

fn parse_claude_log_reader_with_offset<R>(
    reader: &mut R,
    project_path: &str,
    external_session_id: &str,
    aggregate: &mut ClaudeAggregate,
    current_offset: &mut u64,
    allow_incomplete_final_line: bool,
) -> ClaudeParseProgress
where
    R: BufRead,
{
    let mut matched = false;
    let mut has_pending_partial = false;
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(bytes) = reader.read_line(&mut line) else {
            break;
        };
        if bytes == 0 {
            break;
        }
        let has_line_ending = line.ends_with('\n') || line.ends_with('\r');
        if !has_line_ending && !allow_incomplete_final_line {
            has_pending_partial = true;
            break;
        }
        match parse_claude_log_line(&line, project_path, external_session_id, aggregate) {
            Some(line_matched) => {
                *current_offset = current_offset.saturating_add(bytes as u64);
                matched |= line_matched;
            }
            None if has_line_ending => {
                *current_offset = current_offset.saturating_add(bytes as u64);
            }
            None => break,
        }
    }
    ClaudeParseProgress {
        matched,
        has_pending_partial,
    }
}

fn parse_claude_log_line(
    line: &str,
    project_path: &str,
    external_session_id: &str,
    aggregate: &mut ClaudeAggregate,
) -> Option<bool> {
    let Ok(row) = serde_json::from_str::<Value>(line) else {
        return None;
    };
    if row.get("sessionId").and_then(|value| value.as_str()) != Some(external_session_id) {
        return Some(false);
    }
    if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str())
        && !paths_equivalent(Some(cwd), project_path)
    {
        return Some(false);
    }
    if is_claude_control_command_row(&row) {
        return Some(true);
    }
    let timestamp = row
        .get("timestamp")
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
        .unwrap_or_else(now_seconds);
    if let Some(event_timestamp) = row
        .get("timestamp")
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
    {
        aggregate.last_event_at = Some(
            aggregate
                .last_event_at
                .unwrap_or(event_timestamp)
                .max(event_timestamp),
        );
    }
    aggregate.updated_at = aggregate.updated_at.max(timestamp);
    // The current permission mode rides its own row (no message/role, and --
    // unlike message rows -- no timestamp). Capture it for the prompt-wait
    // gate; last one in file order wins.
    if row.get("type").and_then(|value| value.as_str()) == Some("permission-mode") {
        if let Some(mode) = row
            .get("permissionMode")
            .and_then(|value| value.as_str())
            .and_then(|value| normalized_string(Some(value)))
        {
            aggregate.permission_mode = Some(mode);
        }
        return Some(true);
    }
    let message = row.get("message").unwrap_or(&Value::Null);
    let role = message
        .get("role")
        .and_then(|value| value.as_str())
        .or_else(|| row.get("type").and_then(|value| value.as_str()));
    if role == Some("user") {
        // A `tool_result` answers a pending `tool_use`; record it so the
        // pending-call signature clears once the result lands. Tool results
        // are never interruptions, so track them regardless of the branch.
        if claude_message_has_block(message, "tool_result") {
            aggregate.last_tool_result_at = aggregate.last_tool_result_at.max(timestamp);
        }
        if is_claude_interrupted_row(&row) {
            aggregate.last_interrupted_at = aggregate.last_interrupted_at.max(timestamp);
            aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
        } else {
            aggregate.last_user_at = aggregate.last_user_at.max(timestamp);
        }
    } else if role == Some("assistant") {
        if claude_message_has_block(message, "tool_use") {
            aggregate.last_tool_use_at = aggregate.last_tool_use_at.max(timestamp);
        }
        let stop_reason = message.get("stop_reason").and_then(|value| value.as_str());
        if stop_reason == Some("end_turn") {
            aggregate.last_completed_turn_at = aggregate.last_completed_turn_at.max(timestamp);
            aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
        }
        if let Some(preview) =
            sanitized_preview_from_values(&[message.get("content"), row.get("content")])
        {
            aggregate.assistant_preview = Some(preview);
        }
        parse_claude_task_tool_uses(message, timestamp, aggregate);
    } else if role == Some("system") {
        let subtype = row.get("subtype").and_then(|value| value.as_str());
        if matches!(subtype, Some("turn_duration" | "stop_hook_summary")) {
            aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
        }
    }
    parse_claude_task_result(&row, timestamp, aggregate);
    if let Some(model) = first_string_deep(&row, &["model"]) {
        aggregate.model = Some(model);
    }
    if let Some(usage) = first_object_deep(&row, &["usage"]) {
        aggregate.input_tokens += json_i64(usage.get("input_tokens"));
        aggregate.output_tokens += json_i64(usage.get("output_tokens"));
        aggregate.cached_input_tokens += json_i64(usage.get("cache_creation_input_tokens"))
            + json_i64(usage.get("cache_read_input_tokens"));
        aggregate.total_tokens +=
            json_i64(usage.get("input_tokens")) + json_i64(usage.get("output_tokens"));
    }
    Some(true)
}

fn merge_claude_tasks(
    mut left: BTreeMap<String, AIPlanItem>,
    right: BTreeMap<String, AIPlanItem>,
) -> BTreeMap<String, AIPlanItem> {
    for (key, value) in right {
        left.insert(key, value);
    }
    left
}

fn parse_claude_task_tool_uses(message: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
    let Some(content) = message.get("content").and_then(|value| value.as_array()) else {
        return;
    };
    for item in content {
        if item.get("type").and_then(|value| value.as_str()) != Some("tool_use") {
            continue;
        }
        match item.get("name").and_then(|value| value.as_str()) {
            Some("TaskCreate") => parse_claude_task_create(item, timestamp, aggregate),
            Some("TaskUpdate") => parse_claude_task_update(item, timestamp, aggregate),
            _ => {}
        }
    }
}

fn parse_claude_task_create(item: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
    let input = item.get("input").unwrap_or(&Value::Null);
    let Some(text) = input
        .get("subject")
        .and_then(|value| value.as_str())
        .or_else(|| input.get("description").and_then(|value| value.as_str()))
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    let key = input
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| item.get("id").and_then(|value| value.as_str()))
        .and_then(|value| normalized_string(Some(value)))
        .unwrap_or_else(|| format!("pending-{}", aggregate.tasks.len() + 1));
    aggregate.tasks.insert(
        key,
        AIPlanItem {
            text,
            status: "pending".to_string(),
            priority: None,
        },
    );
    aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
}

fn parse_claude_task_update(item: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
    let input = item.get("input").unwrap_or(&Value::Null);
    let Some(task_id) = input
        .get("taskId")
        .and_then(|value| value.as_str())
        .or_else(|| input.get("id").and_then(|value| value.as_str()))
        .or_else(|| item.get("id").and_then(|value| value.as_str()))
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    let status = input
        .get("status")
        .and_then(|value| value.as_str())
        .map(normalized_plan_status)
        .unwrap_or_else(|| "pending".to_string());
    aggregate
        .tasks
        .entry(task_id)
        .and_modify(|task| task.status = status.clone())
        .or_insert(AIPlanItem {
            text: "Task".to_string(),
            status,
            priority: None,
        });
    aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
}

fn parse_claude_task_result(row: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
    if let Some(tasks) = row
        .get("toolUseResult")
        .and_then(|value| value.get("tasks"))
        .and_then(|value| value.as_array())
    {
        let items = tasks
            .iter()
            .filter_map(|task| {
                let text = task
                    .get("subject")
                    .and_then(|value| value.as_str())
                    .and_then(|value| normalized_string(Some(value)))?;
                let status = task
                    .get("status")
                    .and_then(|value| value.as_str())
                    .map(normalized_plan_status)
                    .unwrap_or_else(|| "pending".to_string());
                Some(AIPlanItem {
                    text,
                    status,
                    priority: None,
                })
            })
            .collect::<Vec<_>>();
        if !items.is_empty() {
            aggregate.task_list = Some(items);
            aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
        }
        return;
    }

    let Some(task) = row
        .get("toolUseResult")
        .and_then(|value| value.get("task"))
        .and_then(|value| value.as_object())
    else {
        return;
    };
    let Some(id) = task
        .get("id")
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    let Some(subject) = task
        .get("subject")
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    aggregate.tasks.insert(
        id,
        AIPlanItem {
            text: subject,
            status: "pending".to_string(),
            priority: None,
        },
    );
    aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
}

fn normalized_plan_status(value: &str) -> String {
    match value.trim() {
        "completed" | "complete" | "done" => "completed",
        "in_progress" | "in-progress" | "running" | "active" => "in_progress",
        _ => "pending",
    }
    .to_string()
}

/// A genuine user interruption is recorded by Claude as a `user` row whose
/// message text is exactly its marker -- `[Request interrupted by user]` or
/// `[Request interrupted by user for tool use]`. Match ONLY that marker.
///
/// The previous heuristic stringified the whole row and scanned for
/// "interrupted"/"cancelled"/"aborted" anywhere. Those words are everyday in
/// command/tool output (e.g. "operation cancelled", "connection aborted", "no
/// matches"), so tool-result `user` rows were constantly misread as turn
/// interruptions. That pushed `last_completion_at` past `last_user_at`, flipping
/// `response_state` to idle and demoting a live turn to "completed" -- the
/// session showed no running state even while Claude was clearly working.
fn is_claude_interrupted_row(row: &Value) -> bool {
    claude_user_message_text(row)
        .map(|text| {
            text.trim_start()
                .starts_with("[Request interrupted by user")
        })
        .unwrap_or(false)
}

fn is_claude_control_command_row(row: &Value) -> bool {
    if row.get("type").and_then(Value::as_str) != Some("user") {
        return false;
    }
    // Compact summaries and meta rows are machine-written context, not prompts.
    if row.get("isCompactSummary").and_then(Value::as_bool) == Some(true)
        || row.get("isMeta").and_then(Value::as_bool) == Some(true)
    {
        return true;
    }
    claude_user_message_text(row)
        .map(|text| {
            let text = text.trim_start();
            text.starts_with("<local-command-") || text.starts_with("<command-name>")
        })
        .unwrap_or(false)
}

/// The user message's plain text (string content, or the concatenated `text`
/// blocks of array content). Tool results carry `tool_result` blocks rather than
/// `text`, so they never contribute here -- exactly what keeps their incidental
/// wording from being mistaken for the interrupt marker.
fn claude_user_message_text(row: &Value) -> Option<String> {
    match row.get("message")?.get("content")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if item.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = item.get("text").and_then(Value::as_str)
                {
                    out.push_str(text);
                }
            }
            (!out.is_empty()).then_some(out)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::constants::NEEDS_INPUT_IDLE_SECONDS;
    use std::{collections::HashSet, io::Write};

    const TEST_PROJECT: &str = "/tmp/claude-project";
    const TEST_LAUNCH_AT: f64 = 1_767_225_600.0;

    #[test]
    fn parses_task_list_result_into_plan() {
        let mut aggregate = ClaudeAggregate::default();
        let row = serde_json::json!({
            "toolUseResult": {
                "tasks": [
                    {"id": "a", "subject": "Inspect logs", "status": "completed"},
                    {"id": "b", "subject": "Patch parser", "status": "in_progress"}
                ]
            }
        });

        parse_claude_task_result(&row, 42.0, &mut aggregate);
        let plan = aggregate.plan("claude-session").expect("plan");

        assert_eq!(plan.source, "claude");
        assert_eq!(plan.session_id, "claude-session");
        assert_eq!(plan.updated_at, 42.0);
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.items[0].text, "Inspect logs");
        assert_eq!(plan.items[0].status, "completed");
        assert_eq!(plan.items[1].status, "in_progress");
    }

    fn request_for_claude_resume(external_session_id: Option<&str>) -> AIRuntimeProbeRequest {
        AIRuntimeProbeRequest {
            terminal_id: "terminal-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_path: Some(TEST_PROJECT.to_string()),
            tool: "claude".to_string(),
            external_session_id: external_session_id.map(str::to_string),
            transcript_path: None,
            started_at: Some(TEST_LAUNCH_AT),
            updated_at: TEST_LAUNCH_AT,
            occupied_external_session_ids: Default::default(),
        }
    }

    fn write_claude_transcript(dir: &Path, session_id: &str, timestamp: &str) -> PathBuf {
        let path = dir.join(format!("{session_id}.jsonl"));
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"{session_id}","cwd":"{TEST_PROJECT}","timestamp":"{timestamp}","type":"user","message":{{"role":"user","content":"run"}}}}"#
        )
        .unwrap();
        path
    }

    fn wait_for_distinct_mtime() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    #[test]
    fn missing_bound_id_uses_launch_fresh_cwd_session() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-missing-bound-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let resumed = write_claude_transcript(&dir, "resumed-session", "2026-01-01T00:00:02Z");

        let snapshot = probe_claude_runtime_from_paths(
            &request_for_claude_resume(Some("new-wrapper-session")),
            TEST_PROJECT,
            &[resumed],
            None,
        )
        .expect("snapshot");

        assert_eq!(
            snapshot.external_session_id.as_deref(),
            Some("resumed-session")
        );
        assert_eq!(snapshot.response_state.as_deref(), Some("responding"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_bound_id_wins_when_it_is_freshest() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-bound-freshest-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let older = write_claude_transcript(&dir, "older-session", "2026-01-01T00:00:02Z");
        wait_for_distinct_mtime();
        let bound = write_claude_transcript(&dir, "bound-session", "2026-01-01T00:00:03Z");

        let snapshot = probe_claude_runtime_from_paths(
            &request_for_claude_resume(Some("bound-session")),
            TEST_PROJECT,
            &[older, bound],
            None,
        )
        .expect("snapshot");

        assert_eq!(
            snapshot.external_session_id.as_deref(),
            Some("bound-session")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bound_latest_does_not_scan_older_fallback_candidate() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-bound-no-scan-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let stale = dir.join("stale-session.jsonl");
        std::fs::write(
            &stale,
            r#"{"sessionId":"stale-session","cwd":"/wrong/project","timestamp":"2026-01-01T00:00:04Z","type":"user","message":{"role":"user","content":"wrong"}}"#,
        )
        .unwrap();
        wait_for_distinct_mtime();
        let bound = write_claude_transcript(&dir, "bound-session", "2026-01-01T00:00:05Z");

        let snapshot = probe_claude_runtime_from_paths(
            &request_for_claude_resume(Some("bound-session")),
            TEST_PROJECT,
            &[stale, bound],
            None,
        )
        .expect("snapshot");

        assert_eq!(
            snapshot.external_session_id.as_deref(),
            Some("bound-session")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn newer_resume_session_can_replace_existing_bound_id() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-newer-fallback-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let bound = write_claude_transcript(&dir, "bound-session", "2026-01-01T00:00:02Z");
        wait_for_distinct_mtime();
        let resumed = write_claude_transcript(&dir, "resumed-session", "2026-01-01T00:00:05Z");

        let snapshot = probe_claude_runtime_from_paths(
            &request_for_claude_resume(Some("bound-session")),
            TEST_PROJECT,
            &[bound, resumed],
            None,
        )
        .expect("snapshot");

        assert_eq!(
            snapshot.external_session_id.as_deref(),
            Some("resumed-session")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cold_start_without_bound_or_launch_fresh_file_stays_unresolved() {
        let request = request_for_claude_resume(Some("new-wrapper-session"));

        let snapshot = probe_claude_runtime_from_paths(&request, TEST_PROJECT, &[], None);

        assert!(snapshot.is_none());
    }

    #[test]
    fn launch_bound_rejects_prelaunch_transcript() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-prelaunch-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let stale = write_claude_transcript(&dir, "old-session", "2025-12-31T23:59:00Z");

        let snapshot = probe_claude_runtime_from_paths(
            &request_for_claude_resume(Some("new-wrapper-session")),
            TEST_PROJECT,
            &[stale],
            None,
        );

        assert!(snapshot.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fallback_skips_session_owned_by_other_terminal() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-owned-fallback-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let available = write_claude_transcript(&dir, "available-session", "2026-01-01T00:00:04Z");
        wait_for_distinct_mtime();
        let owned = write_claude_transcript(&dir, "owned-session", "2026-01-01T00:00:06Z");
        let mut request = request_for_claude_resume(Some("new-wrapper-session"));
        request.occupied_external_session_ids = HashSet::from(["owned-session".to_string()]);

        let snapshot =
            probe_claude_runtime_from_paths(&request, TEST_PROJECT, &[owned, available], None)
                .expect("snapshot");

        assert_eq!(
            snapshot.external_session_id.as_deref(),
            Some("available-session")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prelaunch_unfinished_resume_is_silent_idle() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-stale-open-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stale-session.jsonl");
        std::fs::write(
            &path,
            [
                r#"{"sessionId":"stale-session","cwd":"/tmp/claude-project","timestamp":"2025-12-31T23:59:00Z","type":"user","message":{"role":"user","content":"run"}}"#,
                r#"{"sessionId":"stale-session","cwd":"/tmp/claude-project","timestamp":"2025-12-31T23:59:01Z","type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"make"}}]}}"#,
            ]
            .join("\n") + "\n",
        )
        .unwrap();

        let snapshot = probe_claude_runtime_from_paths(
            &request_for_claude_resume(Some("stale-session")),
            TEST_PROJECT,
            &[path],
            None,
        )
        .expect("snapshot");

        assert_eq!(snapshot.response_state.as_deref(), Some("idle"));
        assert_eq!(
            snapshot.source,
            crate::ai_runtime::constants::CLAUDE_STALE_PRELAUNCH_OPEN_TURN_SOURCE
        );
        assert_eq!(snapshot.completed_at, Some(TEST_LAUNCH_AT));
        assert!(!snapshot.was_interrupted);
        assert!(!snapshot.has_completed_turn);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn postlaunch_append_restores_claude_responding() {
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-resume-postlaunch-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fresh-session.jsonl");
        std::fs::write(
            &path,
            r#"{"sessionId":"fresh-session","cwd":"/tmp/claude-project","timestamp":"2026-01-01T00:00:02Z","type":"user","message":{"role":"user","content":"run"}}"#,
        )
        .unwrap();

        let snapshot = probe_claude_runtime_from_paths(
            &request_for_claude_resume(Some("fresh-session")),
            TEST_PROJECT,
            &[path],
            None,
        )
        .expect("snapshot");

        assert_eq!(snapshot.response_state.as_deref(), Some("responding"));
        assert_eq!(snapshot.source, "probe");
        assert!(!snapshot.was_interrupted);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn updates_created_task_status() {
        let mut aggregate = ClaudeAggregate::default();
        let create = serde_json::json!({
            "type": "tool_use",
            "id": "tool-a",
            "name": "TaskCreate",
            "input": {"id": "task-a", "subject": "Write tests"}
        });
        let update = serde_json::json!({
            "type": "tool_use",
            "name": "TaskUpdate",
            "input": {"taskId": "task-a", "status": "completed"}
        });

        parse_claude_task_create(&create, 10.0, &mut aggregate);
        parse_claude_task_update(&update, 11.0, &mut aggregate);
        let plan = aggregate.plan("claude-session").expect("plan");

        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].text, "Write tests");
        assert_eq!(plan.items[0].status, "completed");
        assert_eq!(plan.updated_at, 11.0);
    }

    #[test]
    fn tool_result_wording_is_not_an_interrupt() {
        // Everyday command/tool output mentioning these words must NOT register
        // as a turn interruption.
        let block = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "content": "error: operation cancelled\nbuild aborted"}
            ]}
        });
        let text = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": "why was the deploy aborted?"}
        });
        assert!(!is_claude_interrupted_row(&block));
        assert!(!is_claude_interrupted_row(&text));
    }

    #[test]
    fn genuine_interrupt_marker_is_detected() {
        let string_form = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": "[Request interrupted by user]"}
        });
        let block_form = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "text", "text": "[Request interrupted by user for tool use]"}
            ]}
        });
        assert!(is_claude_interrupted_row(&string_form));
        assert!(is_claude_interrupted_row(&block_form));
    }

    #[test]
    fn live_turn_with_cancel_wording_stays_responding() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("codux-claude-probe-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"do the thing"}}}}"#
        )
        .unwrap();
        // Tool result whose output mentions "cancelled" -- mid-turn activity,
        // not an interruption. The turn must still read as responding.
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:05Z","type":"user","message":{{"role":"user","content":[{{"type":"tool_result","content":"error: operation cancelled"}}]}}}}"#
        )
        .unwrap();
        drop(file);

        let aggregate = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("aggregate");
        assert_eq!(aggregate.response_state().as_deref(), Some("responding"));
        assert!(!aggregate.was_interrupted());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn local_slash_commands_do_not_start_a_runtime_turn() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("codux-claude-local-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"type":"permission-mode","permissionMode":"bypassPermissions","sessionId":"s1"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"<local-command-caveat>Caveat: local command</local-command-caveat>"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"<command-name>/effort</command-name>\n<command-message>effort</command-message>"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"<local-command-stdout>Set effort level to xhigh</local-command-stdout>"}}}}"#
        )
        .unwrap();
        drop(file);

        let aggregate = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("aggregate");

        assert_eq!(aggregate.response_state(), None);
        assert_eq!(aggregate.last_user_at, 0.0);
        assert_eq!(aggregate.last_completion_at, 0.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compact_summary_rows_do_not_start_a_runtime_turn() {
        use std::io::Write;
        let dir =
            std::env::temp_dir().join(format!("codux-claude-compact-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"system","subtype":"compact_boundary"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:01Z","type":"user","isCompactSummary":true,"message":{{"role":"user","content":"This session is being continued from a previous conversation."}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:02Z","type":"user","isMeta":true,"message":{{"role":"user","content":"injected meta context"}}}}"#
        )
        .unwrap();
        drop(file);

        let aggregate = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("aggregate");

        assert_eq!(aggregate.response_state(), None);
        assert_eq!(aggregate.last_user_at, 0.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_tool_use_past_idle_gap_is_a_user_wait() {
        let aggregate = ClaudeAggregate {
            last_user_at: 10.0,
            last_tool_use_at: 12.0,
            ..Default::default()
        };
        assert!(aggregate.pending_tool_call());
        assert!(aggregate.prompts_possible());
        assert_eq!(aggregate.response_state().as_deref(), Some("responding"));
        // Still fresh -> could be a fast auto-approved call, not a wait yet.
        assert!(!aggregate.needs_user_input(12.0 + NEEDS_INPUT_IDLE_SECONDS - 0.5));
        // Idle past the gap -> blocked on the user.
        assert!(aggregate.needs_user_input(12.0 + NEEDS_INPUT_IDLE_SECONDS + 0.5));
    }

    #[test]
    fn bypass_permissions_never_waits() {
        let aggregate = ClaudeAggregate {
            last_user_at: 10.0,
            last_tool_use_at: 12.0,
            permission_mode: Some("bypassPermissions".to_string()),
            ..Default::default()
        };
        assert!(!aggregate.prompts_possible());
        assert!(!aggregate.needs_user_input(1_000.0));
    }

    #[test]
    fn answered_tool_call_is_not_a_wait() {
        let aggregate = ClaudeAggregate {
            last_user_at: 10.0,
            last_tool_use_at: 12.0,
            last_tool_result_at: 13.0,
            ..Default::default()
        };
        assert!(!aggregate.pending_tool_call());
        assert!(!aggregate.needs_user_input(1_000.0));
    }

    #[test]
    fn parses_permission_mode_and_pending_tool_use_from_transcript() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("codux-claude-wait-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        // permission-mode rides its own (timestamp-less) row.
        writeln!(
            file,
            r#"{{"type":"permission-mode","permissionMode":"default","sessionId":"s1"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"run the build"}}}}"#
        )
        .unwrap();
        // Assistant emits a tool_use (stop_reason tool_use), no tool_result follows.
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:02Z","type":"assistant","message":{{"role":"assistant","stop_reason":"tool_use","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"make"}}}}]}}}}"#
        )
        .unwrap();
        drop(file);

        let aggregate = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("aggregate");
        assert_eq!(aggregate.permission_mode.as_deref(), Some("default"));
        assert!(aggregate.pending_tool_call());
        assert!(aggregate.last_tool_use_at > 0.0);
        assert_eq!(aggregate.response_state().as_deref(), Some("responding"));
        assert!(
            aggregate.needs_user_input(aggregate.last_tool_use_at + NEEDS_INPUT_IDLE_SECONDS + 1.0)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn incremental_cache_matches_full_parse() {
        use std::io::Write;
        let dir =
            std::env::temp_dir().join(format!("codux-claude-incremental-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        let first = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{"role":"user","content":"run"}}"#;
        let second = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:02Z","type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"make"}}]},"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":2}}"#;
        let third = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:04Z","type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}"#;
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "{first}").unwrap();
        }
        let mut cache = ClaudeProbeCache::default();
        let first_cached =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("first aggregate");
        assert_eq!(first_cached.last_user_at, 1767225600.0);

        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(file, "{second}").unwrap();
            writeln!(file, "{third}").unwrap();
        }
        let cached =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("cached aggregate");
        let full = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("full aggregate");
        assert_eq!(cached, full);
        assert_eq!(cached.total_tokens, 15);
        assert_eq!(cached.cached_input_tokens, 2);
        assert!(!cached.pending_tool_call());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn incremental_cache_waits_for_complete_jsonl_line() {
        use std::io::Write;
        let dir =
            std::env::temp_dir().join(format!("codux-claude-partial-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        let first = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{"role":"user","content":"run"}}"#;
        let second = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:02Z","type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","content":"done"},"usage":{"input_tokens":1,"output_tokens":2}}"#;
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "{first}").unwrap();
        }
        let mut cache = ClaudeProbeCache::default();
        parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
            .expect("first aggregate");
        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            write!(file, "{}", &second[..second.len() / 2]).unwrap();
        }
        let partial =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("partial aggregate");
        assert_eq!(partial.total_tokens, 0);
        {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(file, "{}", &second[second.len() / 2..]).unwrap();
        }
        let complete =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("complete aggregate");
        assert_eq!(complete.total_tokens, 3);
        assert_eq!(complete.response_state().as_deref(), Some("idle"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn incremental_cache_tracks_consumed_offset_without_replay() {
        let first = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"assistant","message":{"role":"assistant","content":"ok"},"usage":{"input_tokens":1,"output_tokens":1}}"#;
        let second = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:01Z","type":"assistant","message":{"role":"assistant","content":"ok"},"usage":{"input_tokens":2,"output_tokens":2}}"#;
        let mut aggregate = ClaudeAggregate::default();
        let mut current_offset = 0;
        let mut reader = std::io::Cursor::new(format!("{first}\n{second}\n"));

        let progress = parse_claude_log_reader_with_offset(
            &mut reader,
            "/tmp/p",
            "s1",
            &mut aggregate,
            &mut current_offset,
            false,
        );

        assert!(progress.matched);
        assert_eq!(aggregate.total_tokens, 6);
        assert_eq!(current_offset, (first.len() + 1 + second.len() + 1) as u64);
    }

    #[test]
    fn incremental_cache_pending_partial_without_growth_does_not_rebuild() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-pending-no-growth-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        let first = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{"role":"user","content":"run"}}"#;
        let partial = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:02Z","type":"assistant","message":{"role":"assistant","content":"#;
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "{first}").unwrap();
            write!(file, "{partial}").unwrap();
        }

        let mut cache = ClaudeProbeCache::default();
        let first_parse =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("first aggregate");
        assert_eq!(first_parse.last_user_at, 1767225600.0);
        let entry = cache.files.values().next().expect("cache entry");
        let parsed_offset = entry.parsed_offset;
        assert!(entry.has_pending_partial);

        let second_parse =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("second aggregate");
        assert_eq!(second_parse.last_user_at, 1767225600.0);
        let entry = cache.files.values().next().expect("cache entry");
        assert_eq!(entry.parsed_offset, parsed_offset);
        assert_eq!(entry.aggregate.total_tokens, 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn incremental_cache_rebuilds_after_truncation() {
        use std::io::Write;
        let dir =
            std::env::temp_dir().join(format!("codux-claude-truncate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        let first = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{"role":"user","content":"old"}}"#;
        let replacement = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:10Z","type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","content":"new"},"usage":{"input_tokens":4,"output_tokens":6}}"#;
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "{first}").unwrap();
            writeln!(file, "{replacement}").unwrap();
        }
        let mut cache = ClaudeProbeCache::default();
        let original =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("original aggregate");
        assert_eq!(original.total_tokens, 10);

        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "{first}").unwrap();
        }
        let rebuilt =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("rebuilt aggregate");
        assert_eq!(rebuilt.total_tokens, 0);
        assert_eq!(rebuilt.response_state().as_deref(), Some("responding"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn incremental_cache_rebuilds_when_size_is_rewritten_equal() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!(
            "codux-claude-rewrite-same-size-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        let first = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"assistant","message":{"role":"assistant","content":"ok"},"usage":{"input_tokens":1,"output_tokens":1}}"#;
        let second = r#"{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"assistant","message":{"role":"assistant","content":"ok"},"usage":{"input_tokens":2,"output_tokens":2}}"#;
        assert_eq!(first.len(), second.len());
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "{first}").unwrap();
        }
        let mut cache = ClaudeProbeCache::default();
        let original =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("original aggregate");
        assert_eq!(original.total_tokens, 2);

        std::thread::sleep(std::time::Duration::from_millis(5));
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "{second}").unwrap();
        }
        let rebuilt =
            parse_claude_log_runtime_state_cached(&path, "/tmp/p", "s1", "term-1", &mut cache)
                .expect("rebuilt aggregate");
        assert_eq!(rebuilt.total_tokens, 4);
        assert_eq!(
            rebuilt,
            parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("full aggregate")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exact_session_path_is_preferred() {
        let exact = std::path::PathBuf::from("/tmp/s1.jsonl");
        let other = std::path::PathBuf::from("/tmp/other.jsonl");
        assert_eq!(
            claude_log_paths_for_session(&[other.clone(), exact.clone()], "s1"),
            vec![exact]
        );
        assert_eq!(
            claude_log_paths_for_session(std::slice::from_ref(&other), "missing"),
            vec![other]
        );
    }
}
