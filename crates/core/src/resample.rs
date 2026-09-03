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

    let held = held_gears(samples);
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
        grid.push(interpolate_sample(a, b, t, point, held[prev], held[idx]));
    }
    grid
}

fn held_gears(samples: &[TelemetrySample]) -> Vec<i32> {
    let mut held = samples[0].gear;
    samples
        .iter()
        .map(|s| {
            held = crate::hold_transient_gear(held, s.gear);
            held
        })
        .collect()
}

fn interpolate_sample(
    a: &TelemetrySample,
    b: &TelemetrySample,
    t: f32,
    point: usize,
    gear_a: i32,
    gear_b: i32,
) -> DistanceSample {
    DistanceSample {
        distance_pct: point as f32 / (DISTANCE_GRID_POINTS - 1) as f32 * 100.0,
        lap_time_s: lerp(a.lap_time_s, b.lap_time_s, t),
        speed_mps: lerp(a.speed_mps, b.speed_mps, t),
        throttle: lerp(a.throttle, b.throttle, t),
        brake: lerp(a.brake, b.brake, t),
        steering: lerp(a.steering, b.steering, t),
        gear: if t < 1.0 { gear_a as f32 } else { gear_b as f32 },
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
        g_force_x: lerp_opt(a.g_force_x, b.g_force_x, t),
        g_force_y: lerp_opt(a.g_force_y, b.g_force_y, t),
        g_force_z: lerp_opt(a.g_force_z, b.g_force_z, t),
        slip_angle_fl: lerp_opt(a.slip_angle_fl, b.slip_angle_fl, t),
        slip_angle_fr: lerp_opt(a.slip_angle_fr, b.slip_angle_fr, t),
        slip_angle_rl: lerp_opt(a.slip_angle_rl, b.slip_angle_rl, t),
        slip_angle_rr: lerp_opt(a.slip_angle_rr, b.slip_angle_rr, t),
    }
}

fn linear_time_grid(samples: &[TelemetrySample]) -> Vec<DistanceSample> {
    let mut grid = Vec::with_capacity(DISTANCE_GRID_POINTS);
    let last = samples.len() - 1;
    let held = held_gears(samples);
    for point in 0..DISTANCE_GRID_POINTS {
        let t_idx = (point as f32 / (DISTANCE_GRID_POINTS - 1) as f32) * last as f32;
        let prev = t_idx.floor() as usize;
        let next = (prev + 1).min(last);
        let frac = t_idx - prev as f32;
        let a = &samples[prev];
        let b = &samples[next];
        grid.push(interpolate_sample(
            a,
            b,
            frac,
            point,
            held[prev],
            held[next],
        ));
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

/// How much of a lap a trace has to cover before it counts as a whole recording
/// of that lap rather than a fragment of one.
///
/// The two per cent missing is room for the ordinary edges of a recording: the
/// poll that lands just after the start/finish line, and — on ACC — the ~180 ms
/// the adapter holds telemetry back while it waits for the sim to put a time on
/// the crossing. A truncated lap misses far more than that: a physics freeze
/// costs at least ten seconds by definition, and a recording that attached
/// mid-lap misses the whole run up to it.
pub const COMPLETE_TRACE_COVERAGE: f32 = 0.98;

/// A step between consecutive samples longer than this is a hole in the
/// recording rather than sampling jitter. Games are polled at tens of hertz, so
/// an honest step is milliseconds; a second is already orders of magnitude out.
const TRACE_GAP_S: f32 = 1.0;

/// Whether a lap measured at this coverage was recorded whole, and so is worth
/// keeping: a fragment is not a lap of the track, whatever time the sim put on
/// it, and the recorder stores it invalid.
///
/// An unmeasurable trace (`None`) counts as whole. The lap is stored either
/// way, and throwing out laps for a fault the measure could not see is worse
/// than missing one it could.
pub fn trace_is_whole(coverage: Option<f32>) -> bool {
    coverage.is_none_or(|c| c >= COMPLETE_TRACE_COVERAGE)
}

/// The fraction of the lap the recorded trace actually covers, read off the
/// sim's own lap timer.
///
/// Every adapter fills `lap_time_s` from the live lap timer, which resets at the
/// start/finish line, so this measures the same thing on every game: what the
/// trace is missing is the time before its first sample, the time after its
/// last, and any hole in between.
///
/// This is what separates the two reasons a lap can be no good. A lap the sim
/// scored invalid for a cut has a whole trace and still compares perfectly
/// well against a clean lap; a lap whose recording is a fragment does not,
/// because [`resample_to_distance_grid`] stretches that fragment across the
/// full width of the chart. The first is worth drawing, the second is not
/// usable at all — and a single `valid` flag cannot tell them apart.
///
/// `None` when there is nothing to measure against: no lap time, or fewer than
/// two samples.
pub fn trace_coverage(samples: &[TelemetrySample], lap_time_ms: u32) -> Option<f32> {
    coverage_of_lap_times(samples.iter().map(|s| s.lap_time_s), lap_time_ms, true)
}

/// The same measure taken from an already-resampled lap, for laps recorded
/// before the coverage was stored.
///
/// The grid keeps each point's absolute `lap_time_s`, so a trace that started
/// late or stopped early still shows it. What the grid cannot show is a hole in
/// the middle: resampling spaces its points evenly over the distance driven,
/// which closes the gap up. Backfilled laps are measured on their edges alone —
/// enough for the common cases, and never an overstatement of what is missing.
pub fn grid_trace_coverage(grid: &[DistanceSample], lap_time_ms: u32) -> Option<f32> {
    coverage_of_lap_times(grid.iter().map(|s| s.lap_time_s), lap_time_ms, false)
}

/// `count_holes` looks at the steps between samples as well as the edges. Only
/// raw telemetry earns that: a grid's points are spread evenly over the lap, so
/// on a long circuit its own spacing is seconds wide and every step would read
/// as a hole.
fn coverage_of_lap_times(
    times: impl Iterator<Item = f32>,
    lap_time_ms: u32,
    count_holes: bool,
) -> Option<f32> {
    if lap_time_ms == 0 {
        return None;
    }
    let lap_s = lap_time_ms as f32 / 1000.0;
    let times: Vec<f32> = times.collect();
    let (from, to) = lap_run(&times, lap_s)?;
    if to - from < 1 {
        return None;
    }

    let mut holes = 0.0f32;
    if count_holes {
        for i in (from + 1)..=to {
            let step = times[i] - times[i - 1];
            if step > TRACE_GAP_S {
                holes += step;
            }
        }
    }

    let head = times[from].max(0.0);
    let tail = (lap_s - times[to]).max(0.0);
    Some((1.0 - (head + tail + holes) / lap_s).clamp(0.0, 1.0))
}

/// The stretch of the trace belonging to the lap it was recorded for, as an
/// inclusive index range.
///
/// The lap timer resets at the start/finish line, so a step *backwards* means
/// the samples either side of it were timed against different laps. A trace
/// usually holds a handful of those at one end — the sim crosses the line a few
/// polls before the adapter scores the crossing, so the last samples of a lap
/// carry the next lap's timer, reading 0.07 s at the end of a 109 s lap. Taking
/// the trace's final value as its end read those laps as 0.1 % recorded, and
/// they were whole.
///
/// The lap's own run is the one lying inside the lap's own timing window — it
/// starts near zero and ends near the lap time — so the runs are scored on how
/// much of `0..lap_s` they cover. Deliberately *not* the longest run: one real
/// trace opens with two minutes of a stale timer reading 589 s to 712 s before
/// the lap itself starts over at 0.17 s, and length alone picks the stale one.
fn lap_run(times: &[f32], lap_s: f32) -> Option<(usize, usize)> {
    if times.len() < 2 {
        return None;
    }
    let mut best: Option<((usize, usize), f32)> = None;
    let mut start = 0usize;
    for i in 1..=times.len() {
        let broke = i == times.len() || times[i] < times[i - 1];
        if !broke {
            continue;
        }
        let end = i - 1;
        let covered = times[end].min(lap_s) - times[start].max(0.0);
        if best.is_none_or(|(_, best_covered)| covered > best_covered) {
            best = Some(((start, end), covered));
        }
        start = i;
    }
    best.map(|(run, _)| run)
}

/// Metres of track a lap actually covered, from the recorded positions.
///
/// This is driven distance, not spline progress: a lap that leaves the recorded
/// route and rejoins it later measures what the car drove, not the gap it
/// skipped. On the Nurburgring 24h layout that separates a full 25.4 km lap
/// from a joker lap round the GP loop alone, which is 4.6 km of the same spline
/// and crosses the same start/finish line.
pub fn lap_distance_m(samples: &[TelemetrySample]) -> f32 {
    build_cumulative_distance(samples)
        .last()
        .copied()
        .unwrap_or(0.0)
}

/// The same measure taken from an already-resampled lap, for laps recorded
/// before the distance was stored. The grid holds `DISTANCE_GRID_POINTS`
/// positions spaced evenly along the lap, so summing the chords between them
/// understates the true arc by a fraction of a percent — far inside the margin
/// that separates a full lap from a short one.
pub fn grid_distance_m(grid: &[DistanceSample]) -> f32 {
    let mut total = 0.0f32;
    for i in 1..grid.len() {
        let dx = grid[i].pos_x - grid[i - 1].pos_x;
        let dy = grid[i].pos_y - grid[i - 1].pos_y;
        let dz = grid[i].pos_z - grid[i - 1].pos_z;
        total += (dx * dx + dy * dy + dz * dz).sqrt().max(0.0);
    }
    total
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
            g_force_x: None,
            g_force_y: None,
            g_force_z: None,
            slip_angle_fl: None,
            slip_angle_fr: None,
            slip_angle_rl: None,
            slip_angle_rr: None,
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
    fn resample_holds_neutral_gear_blips() {
        let mut samples: Vec<TelemetrySample> = (0..30).map(|i| sample(i, None)).collect();
        for s in &mut samples {
            s.gear = 4;
        }
        samples[10].gear = 1;
        samples[11].gear = 1;
        samples[12].gear = 5;
        for s in samples.iter_mut().skip(13) {
            s.gear = 5;
        }
        let grid = resample_to_distance_grid(&samples);
        assert!(grid.iter().all(|s| s.gear >= 4.0));
        assert!(grid.iter().any(|s| s.gear >= 5.0));
        assert!(grid.iter().all(|s| (s.gear - s.gear.round()).abs() < 1e-5));
    }

    /// A trace running from `from_s` to `to_s` of a lap, sampled at 10 Hz.
    fn trace(from_s: f32, to_s: f32) -> Vec<TelemetrySample> {
        let mut samples = Vec::new();
        let mut t = from_s;
        while t <= to_s + 1e-3 {
            let mut s = sample(0, None);
            s.lap_time_s = t;
            samples.push(s);
            t += 0.1;
        }
        samples
    }

    #[test]
    fn a_whole_trace_covers_its_lap() {
        let coverage = trace_coverage(&trace(0.0, 90.0), 90_000).unwrap();
        assert!(
            coverage >= COMPLETE_TRACE_COVERAGE,
            "a trace over the whole lap reads complete, got {coverage}"
        );
    }

    #[test]
    fn a_trace_that_started_late_measures_short() {
        // The recorder attached a third of the way round: the lap time is the
        // sim's and real, but only two thirds of it was ever recorded.
        let coverage = trace_coverage(&trace(30.0, 90.0), 90_000).unwrap();
        assert!((coverage - 2.0 / 3.0).abs() < 0.01, "got {coverage}");
        assert!(coverage < COMPLETE_TRACE_COVERAGE);
    }

    #[test]
    fn a_trace_that_stopped_early_measures_short() {
        let coverage = trace_coverage(&trace(0.0, 60.0), 90_000).unwrap();
        assert!((coverage - 2.0 / 3.0).abs() < 0.01, "got {coverage}");
    }

    #[test]
    fn a_hole_in_the_middle_measures_short() {
        // A physics freeze from 40 s to 70 s of the lap.
        let mut samples = trace(0.0, 40.0);
        samples.extend(trace(70.0, 90.0));
        let coverage = trace_coverage(&samples, 90_000).unwrap();
        assert!((coverage - 2.0 / 3.0).abs() < 0.01, "got {coverage}");
    }

    #[test]
    fn sampling_jitter_is_not_a_hole() {
        // A slow poll rate is still a whole recording of the lap.
        let samples: Vec<TelemetrySample> = (0..=180)
            .map(|i| {
                let mut s = sample(0, None);
                s.lap_time_s = i as f32 * 0.5;
                s
            })
            .collect();
        let coverage = trace_coverage(&samples, 90_000).unwrap();
        assert!(coverage >= COMPLETE_TRACE_COVERAGE, "got {coverage}");
    }

    #[test]
    fn a_grids_own_spacing_is_not_read_as_holes() {
        // A long lap resampled onto the grid puts seconds between its points.
        // Measured like raw telemetry that would read as one hole after
        // another and call a whole lap a fragment.
        let grid: Vec<DistanceSample> = (0..=200)
            .map(|i| {
                let mut s = interpolate_sample(&sample(0, None), &sample(0, None), 0.0, i, 4, 4);
                s.lap_time_s = i as f32 * 2.5;
                s
            })
            .collect();
        let coverage = grid_trace_coverage(&grid, 500_000).unwrap();
        assert!(coverage >= COMPLETE_TRACE_COVERAGE, "got {coverage}");
    }

    #[test]
    fn samples_from_after_the_line_do_not_shorten_the_lap() {
        // Taken from a real Monza lap: the trace runs the lap out to 109.50 s
        // and then carries four samples timed against the lap that had just
        // started. Reading the last of those as the end of the trace called a
        // whole lap 0.1 % recorded.
        let mut samples = trace(0.32, 109.50);
        samples.extend(trace(0.01, 0.07));
        let coverage = trace_coverage(&samples, 109_515).unwrap();
        assert!(
            coverage >= COMPLETE_TRACE_COVERAGE,
            "the lap was recorded end to end, got {coverage}"
        );
    }

    #[test]
    fn samples_from_before_the_line_do_not_shorten_the_lap() {
        // The same at the other end: a stray tail of the lap before.
        let mut samples = trace(88.0, 88.2);
        samples.extend(trace(0.1, 90.0));
        let coverage = trace_coverage(&samples, 90_000).unwrap();
        assert!(coverage >= COMPLETE_TRACE_COVERAGE, "got {coverage}");
    }

    #[test]
    fn a_reset_does_not_hide_a_fragment() {
        // Strays are dropped, but the run that is left is still measured: this
        // one only starts half way round.
        let mut samples = trace(45.0, 90.0);
        samples.extend(trace(0.01, 0.05));
        let coverage = trace_coverage(&samples, 90_000).unwrap();
        assert!((coverage - 0.5).abs() < 0.02, "got {coverage}");
    }

    #[test]
    fn a_stale_timer_before_the_lap_does_not_win() {
        // Taken from a real Watkins Glen recording: two minutes of a timer left
        // over from an earlier lap, then the lap itself, timed from scratch.
        // Picking the longest run took the stale one and read the lap as never
        // recorded; the lap's own run is the one inside 0..lap_time.
        let mut samples = trace(589.49, 712.12);
        samples.extend(trace(0.17, 106.84));
        let coverage = trace_coverage(&samples, 107_010).unwrap();
        assert!(
            coverage >= COMPLETE_TRACE_COVERAGE,
            "the lap itself was recorded whole, got {coverage}"
        );
    }

    #[test]
    fn a_lap_with_no_time_or_no_trace_is_not_measured() {
        assert_eq!(trace_coverage(&trace(0.0, 90.0), 0), None);
        assert_eq!(trace_coverage(&trace(0.0, 0.0), 90_000), None);
    }

    #[test]
    fn grid_coverage_reads_a_backfilled_lap_off_its_edges() {
        let mut samples = trace(30.0, 90.0);
        for (i, s) in samples.iter_mut().enumerate() {
            s.pos_x = i as f32 * 10.0;
        }
        let grid = resample_to_distance_grid(&samples);
        let coverage = grid_trace_coverage(&grid, 90_000).unwrap();
        assert!((coverage - 2.0 / 3.0).abs() < 0.01, "got {coverage}");
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
