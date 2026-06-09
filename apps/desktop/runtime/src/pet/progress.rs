use super::{
    constants::{MAX_LEVEL, MAX_XP_PER_LEVEL, MIN_XP_PER_LEVEL, TARGET_XP_TO_REACH_LEVEL_100},
    types::PetProgressInfo,
};

pub(super) fn default_persona_id() -> String {
    "observer".to_string()
}

pub(super) fn pet_progress_info(total_xp: i64) -> PetProgressInfo {
    let safe_xp = total_xp.max(0);
    let level = level_from_xp(safe_xp);
    let consumed = total_xp_required_to_reach(level);
    let xp_for_level = xp_for_level(level);
    let xp_in_level = (safe_xp - consumed).max(0);
    PetProgressInfo {
        level,
        xp_in_level,
        xp_for_level,
        total_xp: safe_xp,
        progress: if xp_for_level > 0 {
            (xp_in_level as f64 / xp_for_level as f64).clamp(0.0, 1.0)
        } else {
            1.0
        },
        is_at_max_level: level >= MAX_LEVEL,
    }
}

fn level_from_xp(total_xp: i64) -> i64 {
    let mut level = 1;
    let mut remaining = total_xp.max(0);
    loop {
        let needed = xp_for_level(level);
        if remaining < needed {
            break;
        }
        remaining -= needed;
        level += 1;
    }
    level
}

fn xp_for_level(level: i64) -> i64 {
    let requirements = level_requirements();
    if level >= MAX_LEVEL {
        return requirements.last().copied().unwrap_or(MAX_XP_PER_LEVEL);
    }
    let index = (level - 1).max(0) as usize;
    requirements.get(index).copied().unwrap_or(MAX_XP_PER_LEVEL)
}

fn total_xp_required_to_reach(level: i64) -> i64 {
    if level <= 1 {
        return 0;
    }
    let capped_level = level.min(MAX_LEVEL);
    let capped_index = (capped_level - 2).max(0) as usize;
    let sums = level_prefix_sums();
    let mut total = sums.get(capped_index).copied().unwrap_or(0);
    if level > MAX_LEVEL {
        total += (level - MAX_LEVEL) * xp_for_level(MAX_LEVEL);
    }
    total
}

fn level_requirements() -> Vec<i64> {
    let count = (MAX_LEVEL - 1) as usize;
    let weights = (0..count)
        .map(|index| {
            let progress = if count == 1 {
                0.0
            } else {
                index as f64 / (count - 1) as f64
            };
            MIN_XP_PER_LEVEL as f64 + (MAX_XP_PER_LEVEL - MIN_XP_PER_LEVEL) as f64 * progress
        })
        .collect::<Vec<_>>();
    let weight_total: f64 = weights.iter().sum();
    let mut scaled = weights
        .iter()
        .map(|value| ((value / weight_total) * TARGET_XP_TO_REACH_LEVEL_100 as f64).floor() as i64)
        .collect::<Vec<_>>();
    let remainder = TARGET_XP_TO_REACH_LEVEL_100 - scaled.iter().sum::<i64>();
    for offset in 0..remainder.min(count as i64) {
        let centered_index =
            (((offset as f64 + 0.5) * count as f64) / remainder as f64).floor() as usize;
        scaled[centered_index.min(count - 1)] += 1;
    }
    scaled
}

fn level_prefix_sums() -> Vec<i64> {
    let mut running = 0;
    level_requirements()
        .into_iter()
        .map(|requirement| {
            running += requirement;
            running
        })
        .collect()
}
