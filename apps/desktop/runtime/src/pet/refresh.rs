use super::*;

pub(super) fn refresh_state(state: &mut PetSnapshot, request: PetRefreshInput) {
    let now = now_seconds();
    if state.claimed_at.is_some() {
        let completes_recalibration = state.experience_recalibration_pending;
        state.current_experience_tokens = state
            .experience_base_tokens
            .saturating_add(request.experience_tokens.max(0));
        state.daily_experience_tokens = request.daily_experience_tokens.max(0);
        state.daily_experience_day = Some(day_index(now));
        let computed_stats = request.computed_stats.sanitized();
        if completes_recalibration {
            state.current_stats = computed_stats;
            state.stats_updated_day = Some(now);
            state.experience_recalibration_pending = false;
        } else {
            apply_stats(state, computed_stats, now);
        }
        apply_derived_snapshot_fields(state);
    }
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
