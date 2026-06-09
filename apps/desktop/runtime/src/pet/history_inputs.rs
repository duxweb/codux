use super::*;

pub fn refresh_input_from_indexed_history(
    claimed_at: Option<i64>,
    project_totals: Vec<AIUsageProjectTotal>,
    all_time_total_tokens: i64,
    sessions: Vec<AISessionSummary>,
) -> PetRefreshInput {
    PetRefreshInput {
        project_totals: if claimed_at.is_some() {
            project_totals
                .into_iter()
                .map(|item| PetProjectTokenTotal {
                    project_id: item.project_id,
                    total_tokens: item.total_tokens,
                })
                .collect()
        } else {
            Vec::new()
        },
        fallback_total_tokens: all_time_total_tokens.max(0),
        computed_stats: pet_stats_from_sessions(&sessions),
    }
}
