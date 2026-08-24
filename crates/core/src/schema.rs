use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameId {
    Acc,
    Ac,
    Lmu,
    F1_25,
}

impl GameId {
    pub fn label(&self) -> &'static str {
        match self {
            GameId::Acc => "Assetto Corsa Competizione",
            GameId::Ac => "Assetto Corsa",
            GameId::Lmu => "Le Mans Ultimate",
            GameId::F1_25 => "F1 25",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            GameId::Acc => "ACC",
            GameId::Ac => "AC",
            GameId::Lmu => "LMU",
            GameId::F1_25 => "F1 25",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub game: GameId,
    /// Game-internal track identifier (e.g. ACC `monza`).
    #[serde(default)]
    pub track_id: String,
    pub track: String,
    pub car: String,
    pub game_version: String,
    pub player_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySample {
    pub timestamp: DateTime<Utc>,
    pub lap_time_s: f32,
    pub distance_m: f32,
    pub speed_mps: f32,
    pub throttle: f32,
    pub brake: f32,
    /// ACC: degrees on a ±100° scale (shared-memory input × 100). Other games typically −1…1.
    pub steering: f32,
    pub gear: i32,
    pub rpm: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    /// Liters remaining (ACC physics.fuel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_fl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_fr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_rl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_rr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_fl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_fr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_rl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_rr: Option<f32>,
    /// ACC physics.g_force (x=lat, y=vert, z=long).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g_force_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g_force_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g_force_z: Option<f32>,
    /// ACC physics.slip_angle (degrees per corner).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_fl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_fr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_rl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_rr: Option<f32>,
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorTimes {
    pub s1_ms: Option<u32>,
    pub s2_ms: Option<u32>,
    pub s3_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LapSummary {
    pub lap_number: u32,
    pub lap_time_ms: u32,
    pub valid: bool,
    pub sectors: SectorTimes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_compound: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tc_level: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abs_level: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_used_l: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistanceSample {
    pub distance_pct: f32,
    pub lap_time_s: f32,
    pub speed_mps: f32,
    pub throttle: f32,
    pub brake: f32,
    /// ACC: degrees on a ±100° scale (shared-memory input × 100). Other games typically −1…1.
    pub steering: f32,
    pub gear: f32,
    pub rpm: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_fl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_fr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_rl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_temp_rr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_fl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_fr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_rl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_press_rr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g_force_x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g_force_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g_force_z: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_fl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_fr: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_rl: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_angle_rr: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: Uuid,
    pub game: GameId,
    #[serde(default)]
    pub track_id: String,
    pub track: String,
    pub car: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub game_version: String,
    pub player_name: String,
    pub lap_count: u32,
    pub best_lap_time_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LapRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub lap_number: u32,
    pub lap_time_ms: u32,
    pub valid: bool,
    pub is_best: bool,
    pub is_pinned: bool,
    pub sectors: SectorTimes,
    pub sample_rate_hz: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tyre_compound: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tc_level: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abs_level: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_used_l: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub active: bool,
    pub paused: bool,
    pub game: Option<GameId>,
    pub track: Option<String>,
    pub current_lap: u32,
    pub samples_recorded: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardTrackOption {
    pub track_id: String,
    pub track: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub player_name: String,
    pub lap_time_ms: u32,
    pub valid: bool,
    pub session_id: Uuid,
    pub lap_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackLapOption {
    pub lap_id: Uuid,
    pub session_id: Uuid,
    pub lap_number: u32,
    pub lap_time_ms: u32,
    pub valid: bool,
    pub player_name: String,
    pub car: String,
    pub started_at: DateTime<Utc>,
    pub sectors: SectorTimes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSetupStatus {
    pub game: GameId,
    pub process_detected: bool,
    pub telemetry_active: bool,
    pub message: String,
}

/// Distance-aligned samples written per lap. 4000 marks is ~1.25 m on a 5 km
/// circuit — a 4× bump from the old 1000-point grid so 3 ms capture still
/// improves corner resolution without storing a full time series (~20–30k).
pub const DISTANCE_GRID_POINTS: usize = 4000;

pub const CORE_PARQUET_CHANNELS: &[&str] = &[
    "distance_pct",
    "lap_time_s",
    "speed_mps",
    "throttle",
    "brake",
    "steering",
    "gear",
    "rpm",
    "pos_x",
    "pos_y",
    "pos_z",
];

pub const EXTRA_PARQUET_CHANNELS: &[&str] = &[
    "fuel",
    "tyre_temp_fl",
    "tyre_temp_fr",
    "tyre_temp_rl",
    "tyre_temp_rr",
    "tyre_press_fl",
    "tyre_press_fr",
    "tyre_press_rl",
    "tyre_press_rr",
    "g_force_x",
    "g_force_y",
    "g_force_z",
    "slip_angle_fl",
    "slip_angle_fr",
    "slip_angle_rl",
    "slip_angle_rr",
];

pub fn channel_manifest_json(samples: &[DistanceSample]) -> String {
    let mut channels: Vec<&str> = CORE_PARQUET_CHANNELS.to_vec();
    for name in EXTRA_PARQUET_CHANNELS {
        if samples.iter().any(|s| distance_sample_has_channel(s, name)) {
            channels.push(name);
        }
    }
    serde_json::json!({ "channels": channels }).to_string()
}

pub fn distance_sample_has_channel(sample: &DistanceSample, name: &str) -> bool {
    match name {
        "fuel" => sample.fuel.is_some(),
        "tyre_temp_fl" => sample.tyre_temp_fl.is_some(),
        "tyre_temp_fr" => sample.tyre_temp_fr.is_some(),
        "tyre_temp_rl" => sample.tyre_temp_rl.is_some(),
        "tyre_temp_rr" => sample.tyre_temp_rr.is_some(),
        "tyre_press_fl" => sample.tyre_press_fl.is_some(),
        "tyre_press_fr" => sample.tyre_press_fr.is_some(),
        "tyre_press_rl" => sample.tyre_press_rl.is_some(),
        "tyre_press_rr" => sample.tyre_press_rr.is_some(),
        "g_force_x" => sample.g_force_x.is_some(),
        "g_force_y" => sample.g_force_y.is_some(),
        "g_force_z" => sample.g_force_z.is_some(),
        "slip_angle_fl" => sample.slip_angle_fl.is_some(),
        "slip_angle_fr" => sample.slip_angle_fr.is_some(),
        "slip_angle_rl" => sample.slip_angle_rl.is_some(),
        "slip_angle_rr" => sample.slip_angle_rr.is_some(),
        _ => true,
    }
}

/// ACC/AC expose cumulative elapsed time at the S1 and S2 timing lines.
/// The finish-line `last_sector_time` is per-sector only and must not be stored as S3.
/// Derive per-sector splits from cumulative S1/S2 and official lap time.
pub fn acc_cumulative_splits_to_sectors(
    cum_s1_ms: Option<u32>,
    cum_s2_ms: Option<u32>,
    lap_time_ms: u32,
) -> SectorTimes {
    if lap_time_ms == 0 {
        return SectorTimes {
            s1_ms: None,
            s2_ms: None,
            s3_ms: None,
        };
    }

    match (cum_s1_ms, cum_s2_ms) {
        (Some(s1), Some(s2)) if s2 >= s1 && lap_time_ms >= s2 => SectorTimes {
            s1_ms: Some(s1),
            s2_ms: Some(s2 - s1),
            s3_ms: Some(lap_time_ms - s2),
        },
        (Some(s1), None) if lap_time_ms > s1 => SectorTimes {
            s1_ms: Some(s1),
            s2_ms: None,
            s3_ms: Some(lap_time_ms - s1),
        },
        (None, Some(s2)) if lap_time_ms > s2 => SectorTimes {
            s1_ms: None,
            s2_ms: Some(s2),
            s3_ms: Some(lap_time_ms - s2),
        },
        (None, None) => SectorTimes {
            s1_ms: None,
            s2_ms: None,
            s3_ms: Some(lap_time_ms),
        },
        _ => SectorTimes {
            s1_ms: cum_s1_ms,
            s2_ms: None,
            s3_ms: None,
        },
    }
}

/// ACC/AC expose cumulative split times. Convert to per-sector durations and fill
/// missing S3 from lap time when the finish-line sector transition was missed.
pub fn normalize_sector_times(times: &SectorTimes, lap_time_ms: u32) -> SectorTimes {
    let s1 = times.s1_ms;
    let s2 = times.s2_ms;
    let s3 = times.s3_ms;

    if let (Some(a), Some(b), Some(c)) = (s1, s2, s3) {
        let sum = a + b + c;
        if lap_time_ms > 0
            && sum > 0
            && sum <= lap_time_ms + 500
            && sum >= lap_time_ms.saturating_sub(2000)
        {
            return times.clone();
        }
    }

    // Per-sector with missing S3: sector sum is clearly below lap time (not cumulative S1+S2).
    if let (Some(a), Some(b), None) = (s1, s2, s3) {
        let sum_ab = a + b;
        if lap_time_ms > 0
            && sum_ab < lap_time_ms * 9 / 10
            && lap_time_ms > sum_ab.saturating_add(500)
        {
            return SectorTimes {
                s1_ms: s1,
                s2_ms: s2,
                s3_ms: Some(lap_time_ms - sum_ab),
            };
        }
    }

    let looks_cumulative = matches!((s1, s2), (Some(a), Some(b)) if b > a)
        && lap_time_ms > 0
        && ({
            let sum_ab = s1.unwrap_or(0) + s2.unwrap_or(0);
            sum_ab > lap_time_ms.saturating_sub(lap_time_ms / 10)
                || s2.unwrap_or(0) > lap_time_ms * 2 / 3
        });

    if looks_cumulative {
        let s3_total = s3.unwrap_or(lap_time_ms);
        return SectorTimes {
            s1_ms: s1,
            s2_ms: match (s1, s2) {
                (Some(a), Some(b)) if b >= a => Some(b - a),
                _ => None,
            },
            s3_ms: match s2 {
                Some(b) if s3_total >= b => Some(s3_total - b),
                _ => None,
            },
        };
    }

    if s3.is_none() && lap_time_ms > 0 {
        let prior_ms = match (s1, s2) {
            (Some(a), Some(b)) => a + b,
            (Some(a), None) => a,
            _ => 0,
        };
        if lap_time_ms > prior_ms {
            return SectorTimes {
                s1_ms: s1,
                s2_ms: s2,
                s3_ms: Some(lap_time_ms - prior_ms),
            };
        }
    }

    times.clone()
}

#[cfg(test)]
mod sector_normalize_tests {
    use super::*;

    #[test]
    fn cumulative_acc_splits_to_per_sector() {
        let raw = SectorTimes {
            s1_ms: Some(30_000),
            s2_ms: Some(65_000),
            s3_ms: None,
        };
        let norm = normalize_sector_times(&raw, 100_000);
        assert_eq!(norm.s1_ms, Some(30_000));
        assert_eq!(norm.s2_ms, Some(35_000));
        assert_eq!(norm.s3_ms, Some(35_000));
    }

    #[test]
    fn per_sector_lmu_preserved() {
        let raw = SectorTimes {
            s1_ms: Some(30_000),
            s2_ms: Some(35_000),
            s3_ms: Some(35_000),
        };
        let norm = normalize_sector_times(&raw, 100_000);
        assert_eq!(norm.s1_ms, Some(30_000));
        assert_eq!(norm.s2_ms, Some(35_000));
        assert_eq!(norm.s3_ms, Some(35_000));
    }

    #[test]
    fn fills_missing_s3_per_sector_lap() {
        let raw = SectorTimes {
            s1_ms: Some(30_000),
            s2_ms: Some(35_000),
            s3_ms: None,
        };
        let norm = normalize_sector_times(&raw, 100_000);
        assert_eq!(norm.s1_ms, Some(30_000));
        assert_eq!(norm.s2_ms, Some(35_000));
        assert_eq!(norm.s3_ms, Some(35_000));
    }

    #[test]
    fn per_sector_s2_greater_than_s1_not_cumulative() {
        let raw = SectorTimes {
            s1_ms: Some(28_000),
            s2_ms: Some(38_000),
            s3_ms: None,
        };
        let norm = normalize_sector_times(&raw, 100_000);
        assert_eq!(norm.s1_ms, Some(28_000));
        assert_eq!(norm.s2_ms, Some(38_000));
        assert_eq!(norm.s3_ms, Some(34_000));
    }

    #[test]
    fn acc_cumulative_splits_match_in_game_sectors() {
        let sectors = acc_cumulative_splits_to_sectors(Some(35_072), Some(72_500), 109_900);
        assert_eq!(sectors.s1_ms, Some(35_072));
        assert_eq!(sectors.s2_ms, Some(37_428));
        assert_eq!(sectors.s3_ms, Some(37_400));
    }

    #[test]
    fn watkins_glen_wrong_index_pattern_is_not_valid_conversion() {
        // Pre-fix bug: leaving sector index 1 stored cum-S2 as s1; s2 missing.
        let broken = acc_cumulative_splits_to_sectors(Some(65_525), None, 107_225);
        assert_eq!(broken.s1_ms, Some(65_525));
        assert_eq!(broken.s2_ms, None);
        assert_eq!(broken.s3_ms, Some(41_700));

        // Correct 0-based capture: cum S1 + cum S2 → per-sector match.
        let fixed = acc_cumulative_splits_to_sectors(Some(35_000), Some(65_525), 107_225);
        assert_eq!(fixed.s1_ms, Some(35_000));
        assert_eq!(fixed.s2_ms, Some(30_525));
        assert_eq!(fixed.s3_ms, Some(41_700));
    }
}
