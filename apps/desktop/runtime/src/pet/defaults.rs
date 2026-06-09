use super::*;

impl Default for PetProgressInfo {
    fn default() -> Self {
        pet_progress_info(0)
    }
}

impl Default for PetSnapshot {
    fn default() -> Self {
        Self {
            state_version: STATE_VERSION,
            stats_model_version: STATS_MODEL_VERSION,
            claimed_at: None,
            species: "voidcat".to_string(),
            custom_pet: None,
            custom_name: String::new(),
            current_experience_tokens: 0,
            current_stats: PetStats::default(),
            persona_id: default_persona_id(),
            progress: PetProgressInfo::default(),
            stats_updated_day: None,
            global_normalized_total_watermark: None,
            project_normalized_token_watermarks: HashMap::new(),
            total_normalized_tokens: 0,
            daily_experience_tokens: 0,
            daily_experience_day: None,
            legacy: Vec::new(),
            updated_at: now_seconds(),
        }
    }
}
