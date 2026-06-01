use super::*;

pub(super) fn refresh_state(state: &mut PetSnapshot, request: PetRefreshInput) {
    let now = now_seconds();
    reset_daily_tokens_if_needed(state, now);
    let project_totals = sanitize_project_totals(request.project_totals);
    let fallback_total = request.fallback_total_tokens.max(0);
    let total_normalized_tokens = if project_totals.is_empty() {
        fallback_total
    } else {
        project_totals.values().sum()
    };

    if state.claimed_at.is_none() {
        state.total_normalized_tokens = total_normalized_tokens;
        state.updated_at = now;
        return;
    }

    let delta_tokens = if project_totals.is_empty() {
        let previous = state
            .global_normalized_total_watermark
            .unwrap_or(fallback_total)
            .max(0);
        let delta = (fallback_total - previous).max(0);
        if state.global_normalized_total_watermark.is_none() || fallback_total > previous {
            state.global_normalized_total_watermark = Some(fallback_total);
        }
        delta
    } else {
        let current_project_ids = project_totals.keys().cloned().collect::<Vec<_>>();
        state
            .project_normalized_token_watermarks
            .retain(|project_id, _| current_project_ids.contains(project_id));
        let mut delta = 0;
        for (project_id, total) in &project_totals {
            let previous = state
                .project_normalized_token_watermarks
                .get(project_id)
                .copied()
                .unwrap_or(*total);
            delta += (*total - previous).max(0);
            if *total > previous
                || !state
                    .project_normalized_token_watermarks
                    .contains_key(project_id)
            {
                state
                    .project_normalized_token_watermarks
                    .insert(project_id.clone(), *total);
            }
        }
        state.global_normalized_total_watermark = Some(
            state
                .project_normalized_token_watermarks
                .values()
                .copied()
                .sum(),
        );
        delta
    };

    if delta_tokens > 0 {
        state.current_experience_tokens = state
            .current_experience_tokens
            .saturating_add(delta_tokens)
            .max(0);
        state.daily_experience_tokens = state
            .daily_experience_tokens
            .saturating_add(delta_tokens)
            .max(0);
    }
    apply_stats(state, request.computed_stats.sanitized(), now);
    state.total_normalized_tokens = total_normalized_tokens;
    apply_derived_snapshot_fields(state);
    state.updated_at = now;
}

pub(super) fn reset_daily_tokens_if_needed(state: &mut PetSnapshot, now: i64) {
    let day = day_index(now);
    if state.daily_experience_day == Some(day) {
        return;
    }
    state.daily_experience_day = Some(day);
    state.daily_experience_tokens = 0;
}

pub(super) fn apply_derived_snapshot_fields(state: &mut PetSnapshot) {
    state.persona_id = pet_persona_id(&state.current_stats).to_string();
    state.progress = pet_progress_info(state.current_experience_tokens);
    for record in &mut state.legacy {
        record.persona_id = pet_persona_id(&record.stats).to_string();
        record.progress = pet_progress_info(record.total_xp);
    }
}

pub(super) fn sanitize_project_totals(items: Vec<PetProjectTokenTotal>) -> HashMap<String, i64> {
    let mut totals = HashMap::new();
    for item in items {
        let project_id = item.project_id.trim();
        if project_id.is_empty() {
            continue;
        }
        totals.insert(project_id.to_string(), item.total_tokens.max(0));
    }
    totals
}
