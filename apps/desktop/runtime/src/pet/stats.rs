use super::{
    constants::STATS_REFRESH_INTERVAL_SECONDS,
    types::{PetSnapshot, PetStats},
};
use crate::ai_history_normalized::AISessionSummary;
use chrono::{Local, TimeZone, Timelike};

impl PetStats {
    fn max_value(&self) -> i64 {
        [
            self.wisdom,
            self.chaos,
            self.night,
            self.stamina,
            self.empathy,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    pub(super) fn sanitized(mut self) -> Self {
        self.wisdom = self.wisdom.max(0);
        self.chaos = self.chaos.max(0);
        self.night = self.night.max(0);
        self.stamina = self.stamina.max(0);
        self.empathy = self.empathy.max(0);
        self
    }
}

pub(super) fn apply_stats(state: &mut PetSnapshot, computed: PetStats, now: i64) {
    if computed.max_value() <= 0 {
        return;
    }
    if state.stats_updated_day.is_none() || state.current_stats == PetStats::default() {
        state.current_stats = computed;
        state.stats_updated_day = Some(now);
        return;
    }
    let should_damp = state
        .stats_updated_day
        .map(|updated_at| now - updated_at >= STATS_REFRESH_INTERVAL_SECONDS)
        .unwrap_or(true);
    if should_damp && state.current_stats != computed {
        state.current_stats = damp_stats(&state.current_stats, &computed);
        state.stats_updated_day = Some(now);
    }
}

fn damp_stats(current: &PetStats, target: &PetStats) -> PetStats {
    fn damp(current: i64, next: i64) -> i64 {
        let delta = ((next - current) as f64 * 0.25).round() as i64;
        if delta == 0 && current != next {
            return (current + if next > current { 1 } else { -1 }).max(0);
        }
        (current + delta).max(0)
    }
    PetStats {
        wisdom: damp(current.wisdom, target.wisdom),
        chaos: damp(current.chaos, target.chaos),
        night: damp(current.night, target.night),
        stamina: damp(current.stamina, target.stamina),
        empathy: damp(current.empathy, target.empathy),
    }
}

pub(super) fn pet_stats_from_sessions(sessions: &[AISessionSummary]) -> PetStats {
    if sessions.is_empty() {
        return PetStats::default();
    }

    let total_requests: i64 = sessions.iter().map(|session| session.request_count).sum();
    let total_tokens: i64 = sessions.iter().map(|session| session.total_tokens).sum();
    let measured_sessions = sessions
        .iter()
        .filter(|session| session.active_duration_seconds > 0)
        .collect::<Vec<_>>();
    let total_secs: i64 = measured_sessions
        .iter()
        .map(|session| session.active_duration_seconds)
        .sum();
    let measured_requests: i64 = measured_sessions
        .iter()
        .map(|session| session.request_count)
        .sum();
    let session_count = sessions.len().max(1);

    let avg_tok_per_req = if total_requests > 0 {
        total_tokens as f64 / total_requests as f64
    } else {
        0.0
    };
    let req_per_hour = if total_secs > 0 {
        measured_requests as f64 / (total_secs as f64 / 3600.0)
    } else {
        0.0
    };
    let short_count = measured_sessions
        .iter()
        .filter(|session| session.active_duration_seconds < 300)
        .count();
    let night_count = sessions
        .iter()
        .filter(|session| {
            timestamp_hour_local(session.first_seen_at)
                .map(|hour| !(6..22).contains(&hour))
                .unwrap_or(false)
        })
        .count();
    let sustained_seconds = measured_sessions
        .iter()
        .map(|session| session.active_duration_seconds)
        .collect::<Vec<_>>();
    let max_secs = sustained_seconds.iter().copied().max().unwrap_or(0);
    let multi_turn_sessions = sessions
        .iter()
        .filter(|session| session.request_count >= 4)
        .count();
    let repair_sessions = sessions.iter().filter(|session| {
        if session.request_count < 3 || session.total_tokens <= 0 {
            return false;
        }
        let avg_per_turn = session.total_tokens as f64 / session.request_count as f64;
        session.active_duration_seconds >= 360 && (120.0..=4200.0).contains(&avg_per_turn)
    });
    let repair_token_budget: i64 = repair_sessions.map(|session| session.total_tokens).sum();

    fn smoothed_ratio(positive: usize, total: usize) -> f64 {
        (positive as f64 + 2.0) / (total as f64 + 4.0)
    }
    fn sat_ratio(value: f64, target: f64) -> f64 {
        if value > 0.0 && target > 0.0 {
            value / (value + target)
        } else {
            0.0
        }
    }
    fn display_pts(ratio: f64, weight: f64, exponent: f64) -> f64 {
        if ratio > 0.0 && weight > 0.0 {
            ratio.clamp(0.0, 1.0).powf(exponent).min(1.0) * weight
        } else {
            0.0
        }
    }

    let depth = display_pts(sat_ratio(avg_tok_per_req, 6000.0), 230.0, 0.6);
    let deep_sessions = sessions
        .iter()
        .filter(|session| {
            session.request_count > 0
                && session.total_tokens as f64 / session.request_count as f64 >= 2000.0
        })
        .count();
    let focus = display_pts(smoothed_ratio(deep_sessions, sessions.len()), 80.0, 0.55);
    let burst = display_pts(
        if measured_sessions.is_empty() {
            0.0
        } else {
            smoothed_ratio(short_count, measured_sessions.len())
        },
        200.0,
        0.55,
    );
    let rate = display_pts(sat_ratio(req_per_hour, 6.0), 130.0, 0.65);
    let core = display_pts(smoothed_ratio(night_count, session_count), 240.0, 0.55);
    let streak = display_pts(sat_ratio(night_count as f64, 8.0), 70.0, 0.6);
    let long_count = sustained_seconds
        .iter()
        .filter(|seconds| **seconds >= 1800)
        .count();
    let long = display_pts(
        if measured_sessions.is_empty() {
            0.0
        } else {
            smoothed_ratio(long_count, measured_sessions.len())
        },
        200.0,
        0.55,
    );
    let peak = display_pts(sat_ratio(max_secs as f64, 3600.0), 130.0, 0.6);
    let repair_share = if total_tokens > 0 {
        repair_token_budget as f64 / total_tokens as f64
    } else {
        0.0
    };
    let repair = display_pts(repair_share.min(1.0), 210.0, 0.55);
    let collaboration = display_pts(
        smoothed_ratio(multi_turn_sessions, session_count),
        120.0,
        0.55,
    );

    PetStats {
        wisdom: (depth + focus).round().max(0.0) as i64,
        chaos: (burst + rate).round().max(0.0) as i64,
        night: (core + streak).round().max(0.0) as i64,
        stamina: (long + peak).round().max(0.0) as i64,
        empathy: (repair + collaboration).round().max(0.0) as i64,
    }
}

fn timestamp_hour_local(seconds: f64) -> Option<u32> {
    if !seconds.is_finite() {
        return None;
    }
    Local
        .timestamp_opt(seconds.floor() as i64, 0)
        .single()
        .map(|date| date.hour())
}

pub(super) fn pet_persona_id(stats: &PetStats) -> &'static str {
    let mut values = [
        ("wisdom", stats.wisdom),
        ("chaos", stats.chaos),
        ("night", stats.night),
        ("stamina", stats.stamina),
        ("empathy", stats.empathy),
    ];
    values.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    let strongest = values[0];
    let second = values.get(1).map(|item| item.1).unwrap_or(0);
    if strongest.1 <= 0 {
        return "observer";
    }
    let dominant_gap = strongest.1 - second;
    let mean = values.iter().map(|item| item.1).sum::<i64>() as f64 / values.len() as f64;
    if (strongest.1 as f64) < mean * 1.15 {
        return "balanced";
    }
    if strongest.0 == "wisdom"
        && stats.wisdom >= (stats.chaos + 60).max((second as f64 * 1.18) as i64)
    {
        return if stats.night >= (stats.wisdom as f64 * 0.72) as i64 {
            "midnight_thinker"
        } else {
            "philosopher"
        };
    }
    if strongest.0 == "chaos" && stats.stamina >= (stats.chaos as f64 * 0.7) as i64 {
        return "mad_scientist";
    }
    if strongest.0 == "night" && stats.empathy >= (stats.night as f64 * 0.55) as i64 {
        return "night_companion";
    }
    if strongest.0 == "stamina" && stats.empathy >= (stats.stamina as f64 * 0.6) as i64 {
        return "debug_comrade";
    }
    if strongest.0 == "night" {
        return "night_owl";
    }
    if strongest.0 == "chaos" {
        return if dominant_gap > 40 {
            "firebrand"
        } else {
            "action_seeker"
        };
    }
    if strongest.0 == "stamina" {
        return if dominant_gap > 40 {
            "marathoner"
        } else {
            "steady_type"
        };
    }
    if strongest.0 == "empathy" {
        return "debug_buddy";
    }
    if strongest.0 == "wisdom" {
        return "wise_type";
    }
    "observer"
}
