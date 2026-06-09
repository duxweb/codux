pub(super) const STATE_VERSION: u32 = 8;
pub(super) const STATS_MODEL_VERSION: u32 = 3;
pub(super) const STATS_REFRESH_INTERVAL_SECONDS: i64 = 3600;
pub(super) const PET_STATE_CRYPTO_NAMESPACE: &str = "codux";
pub(super) const MAX_LEVEL: i64 = 100;
pub(super) const DAILY_TARGET_XP: i64 = 40_000_000;
pub(super) const TARGET_XP_TO_REACH_LEVEL_100: i64 = DAILY_TARGET_XP * 30;
pub(super) const MIN_XP_PER_LEVEL: i64 = 2_000_000;
pub(super) const MAX_XP_PER_LEVEL: i64 = 22_000_000;
pub(super) const PET_STATE_DECODE_NAMESPACES: &[&str] = &["codux", "codux-tauri", "prod", "dev"];
pub(super) const PET_SPECIES: &[&str] = &[
    "voidcat",
    "rusthound",
    "goose",
    "chaossprite",
    "code",
    "sheep",
    "ox",
    "dragon",
    "phoenix",
    "dolphin",
    "penguin",
    "panda",
];
pub(super) const CUSTOM_SPECIES_PREFIX: &str = "custom:";
