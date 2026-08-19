use crate::schema::{DistanceSample, TelemetrySample, DISTANCE_GRID_POINTS};

pub fn resample_to_distance_grid(samples: &[TelemetrySample]) -> Vec<DistanceSample> {
    if samples.len() < 2 {
        return Vec::new();
    }

    let cumulative = build_cumulative_distance(samples);
    let total_distance = cumulative.last().copied().unwrap_or(0.0);
    if total_distance <= 0.0 {
        return linear_time_grid(samples);
    }

    let mut grid = Vec::with_capacity(DISTANCE_GRID_POINTS);
    for point in 0..DISTANCE_GRID_POINTS {
        let target = (point as f32 / (DISTANCE_GRID_POINTS - 1) as f32) * total_distance;
        let idx = cumulative
            .iter()
            .position(|&d| d >= target)
            .unwrap_or(samples.len() - 1)
            .max(1);
        let prev = idx - 1;
        let span = (cumulative[idx] - cumulative[prev]).max(f32::EPSILON);
        let t = ((target - cumulative[prev]) / span).clamp(0.0, 1.0);
        let a = &samples[prev];
        let b = &samples[idx];
        grid.push(interpolate_sample(a, b, t, point));
    }
    grid
}

fn interpolate_sample(
    a: &TelemetrySample,
    b: &TelemetrySample,
    t: f32,
    point: usize,
) -> DistanceSample {
    DistanceSample {
        distance_pct: point as f32 / (DISTANCE_GRID_POINTS - 1) as f32 * 100.0,
        lap_time_s: lerp(a.lap_time_s, b.lap_time_s, t),
        speed_mps: lerp(a.speed_mps, b.speed_mps, t),
        throttle: lerp(a.throttle, b.throttle, t),
        brake: lerp(a.brake, b.brake, t),
        steering: lerp(a.steering, b.steering, t),
        gear: lerp(a.gear as f32, b.gear as f32, t),
        rpm: lerp(a.rpm, b.rpm, t),
        pos_x: lerp(a.pos_x, b.pos_x, t),
        pos_y: lerp(a.pos_y, b.pos_y, t),
        pos_z: lerp(a.pos_z, b.pos_z, t),
        fuel: lerp_opt(a.fuel, b.fuel, t),
        tyre_temp_fl: lerp_opt(a.tyre_temp_fl, b.tyre_temp_fl, t),
        tyre_temp_fr: lerp_opt(a.tyre_temp_fr, b.tyre_temp_fr, t),
        tyre_temp_rl: lerp_opt(a.tyre_temp_rl, b.tyre_temp_rl, t),
        tyre_temp_rr: lerp_opt(a.tyre_temp_rr, b.tyre_temp_rr, t),
        tyre_press_fl: lerp_opt(a.tyre_press_fl, b.tyre_press_fl, t),
        tyre_press_fr: lerp_opt(a.tyre_press_fr, b.tyre_press_fr, t),
        tyre_press_rl: lerp_opt(a.tyre_press_rl, b.tyre_press_rl, t),
        tyre_press_rr: lerp_opt(a.tyre_press_rr, b.tyre_press_rr, t),
    }
}

fn linear_time_grid(samples: &[TelemetrySample]) -> Vec<DistanceSample> {
    let mut grid = Vec::with_capacity(DISTANCE_GRID_POINTS);
    for point in 0..DISTANCE_GRID_POINTS {
        let t_idx = (point as f32 / (DISTANCE_GRID_POINTS - 1) as f32) * (samples.len() - 1) as f32;
        let idx = t_idx.floor() as usize;
        let next = idx.min(samples.len() - 1);
        let prev = idx.saturating_sub(0).min(next);
        let frac = t_idx - prev as f32;
        let a = &samples[prev];
        let b = &samples[next];
        grid.push(interpolate_sample(a, b, frac, point));
    }
    grid
}

pub fn compute_fuel_used_l(samples: &[TelemetrySample]) -> Option<f32> {
    let first = samples.first()?.fuel?;
    let last = samples.last()?.fuel?;
    Some((first - last).max(0.0))
}

pub fn compute_time_delta(reference: &[DistanceSample], compare: &[DistanceSample]) -> Vec<f32> {
    let len = reference.len().min(compare.len());
    (0..len)
        .map(|i| compare[i].lap_time_s - reference[i].lap_time_s)
        .collect()
}

pub fn compute_sector_deltas(reference: &crate::schema::SectorTimes, compare: &crate::schema::SectorTimes) -> [Option<i32>; 3] {
    [
        match (reference.s1_ms, compare.s1_ms) {
            (Some(r), Some(c)) => Some(c as i32 - r as i32),
            _ => None,
        },
        match (reference.s2_ms, compare.s2_ms) {
            (Some(r), Some(c)) => Some(c as i32 - r as i32),
            _ => None,
        },
        match (reference.s3_ms, compare.s3_ms) {
            (Some(r), Some(c)) => Some(c as i32 - r as i32),
            _ => None,
        },
    ]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_opt(a: Option<f32>, b: Option<f32>, t: f32) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(lerp(a, b, t)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn build_cumulative_distance(samples: &[TelemetrySample]) -> Vec<f32> {
    let mut cumulative = vec![0.0f32; samples.len()];
    for i in 1..samples.len() {
        let dx = samples[i].pos_x - samples[i - 1].pos_x;
        let dy = samples[i].pos_y - samples[i - 1].pos_y;
        let dz = samples[i].pos_z - samples[i - 1].pos_z;
        let step = (dx * dx + dy * dy + dz * dz).sqrt();
        cumulative[i] = cumulative[i - 1] + step.max(0.0);
    }

    let position_distance = cumulative.last().copied().unwrap_or(0.0);
    if position_distance > 0.0 {
        return cumulative;
    }

    for i in 1..samples.len() {
        let delta = (samples[i].distance_m - samples[i - 1].distance_m).max(0.0);
        cumulative[i] = cumulative[i - 1] + delta;
    }

    cumulative
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample(i: i32, fuel: Option<f32>) -> TelemetrySample {
        TelemetrySample {
            timestamp: Utc::now(),
            lap_time_s: i as f32 * 0.1,
            distance_m: i as f32,
            speed_mps: 50.0,
            throttle: 0.5,
            brake: 0.0,
            steering: 0.0,
            gear: 4,
            rpm: 7000.0,
            pos_x: i as f32,
            pos_y: 0.0,
            pos_z: 0.0,
            fuel,
            tyre_temp_fl: Some(80.0 + i as f32),
            tyre_temp_fr: None,
            tyre_temp_rl: None,
            tyre_temp_rr: None,
            tyre_press_fl: None,
            tyre_press_fr: None,
            tyre_press_rl: None,
            tyre_press_rr: None,
            raw: serde_json::json!({}),
        }
    }

    #[test]
    fn resample_produces_grid() {
        let samples: Vec<TelemetrySample> = (0..100).map(|i| sample(i, None)).collect();
        let grid = resample_to_distance_grid(&samples);
        assert_eq!(grid.len(), DISTANCE_GRID_POINTS);
    }

    #[test]
    fn resample_uses_distance_m_when_position_is_degenerate() {
        let samples: Vec<TelemetrySample> = (0..100)
            .map(|i| {
                let mut s = sample(i, None);
                s.distance_m = i as f32 * 10.0;
                s.pos_x = 0.0;
                s.pos_y = 0.0;
                s.pos_z = 0.0;
                s
            })
            .collect();
        let grid = resample_to_distance_grid(&samples);
        assert_eq!(grid.len(), DISTANCE_GRID_POINTS);
        assert!(grid.last().unwrap().distance_pct > 99.0);
    }

    #[test]
    fn resample_preserves_optional_channels() {
        let samples: Vec<TelemetrySample> = (0..100)
            .map(|i| sample(i, Some(50.0 - i as f32 * 0.1)))
            .collect();
        let grid = resample_to_distance_grid(&samples);
        assert!(grid.iter().all(|s| s.fuel.is_some()));
        assert!(grid.iter().all(|s| s.tyre_temp_fl.is_some()));
    }

    #[test]
    fn fuel_used_clamps_negative() {
        let samples = vec![
            sample(0, Some(10.0)),
            sample(1, Some(12.0)),
        ];
        assert_eq!(compute_fuel_used_l(&samples), Some(0.0));
    }

    #[test]
    fn fuel_used_from_first_and_last() {
        let samples = vec![
            sample(0, Some(50.0)),
            sample(1, Some(42.5)),
        ];
        assert_eq!(compute_fuel_used_l(&samples), Some(7.5));
    }
}
