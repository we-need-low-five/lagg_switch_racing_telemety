use anyhow::Result;
use parking_lot::Mutex;
use sim_capture_acc::AccAdapter;
use sim_capture_ac::AcAdapter;
use sim_capture_f1::F1Adapter;
use sim_capture_lmu::LmuAdapter;
use sim_core::{
    channel_manifest_json, compute_fuel_used_l, lap_distance_m, resample_to_distance_grid,
    session_car_changed, session_kind_changed, session_track_changed, trace_coverage,
    AdapterEvent, GameAdapter, GameId, RecordingStatus, SessionInfo, SessionKind,
    TelemetrySample,
};
use sim_storage::{resolve_data_relative, write_lap_parquet, Database};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::detect_running_game;

const LIVE_PHYSICS_TIMEOUT: Duration = Duration::from_secs(10);

/// Hard cap on a session with no live physics. A stint gap keeps the session
/// open indefinitely for a pause menu; once this elapses we assume the game was
/// abandoned (quit to menu, closed, crashed) and finalize instead.
const SESSION_ABANDON_TIMEOUT: Duration = Duration::from_secs(8 * 60);

fn far_future() -> Instant {
    Instant::now() + Duration::from_secs(365 * 24 * 3600)
}

pub struct RecordingService {
    db: Arc<Mutex<Database>>,
    data_dir: PathBuf,
    adapter: Option<Box<dyn GameAdapter>>,
    session_id: Option<Uuid>,
    current_lap_samples: Vec<TelemetrySample>,
    current_lap_number: u32,
    paused: bool,
    samples_recorded: u64,
    last_tick: Instant,
    sample_rate_estimate: f32,
    session_info: Option<SessionInfo>,
    heartbeat_deadline: Instant,
    abandon_deadline: Instant,
    last_live_physics: Instant,
    current_stint: u32,
    /// Weekend phase the current stint belongs to. Set from the session's first
    /// `SessionInfo` and updated when the sim moves Practice → Qualifying → Race
    /// on the same track and car (that phase change rolls a new stint rather
    /// than a new session). Stamped on every lap of the stint.
    current_stint_kind: SessionKind,
    laps_in_current_stint: bool,
    stint_gap_open: bool,
    stint_gap_during_lap: bool,
    /// Set when a gap that split the stint is waiting on a resume to measure the
    /// break; the break seconds land in `pending_stint_break`.
    stint_break_armed: bool,
    /// Break length to stamp on the first persisted lap of the new stint.
    pending_stint_break: Option<u32>,
}

impl RecordingService {
    pub fn new(db: Arc<Mutex<Database>>, data_dir: PathBuf) -> Self {
        Self {
            db,
            data_dir,
            adapter: None,
            session_id: None,
            current_lap_samples: Vec::new(),
            current_lap_number: 1,
            paused: false,
            samples_recorded: 0,
            last_tick: Instant::now(),
            sample_rate_estimate: 0.0,
            session_info: None,
            heartbeat_deadline: far_future(),
            abandon_deadline: far_future(),
            last_live_physics: Instant::now(),
            current_stint: 1,
            current_stint_kind: SessionKind::Unknown,
            laps_in_current_stint: false,
            stint_gap_open: false,
            stint_gap_during_lap: false,
            stint_break_armed: false,
            pending_stint_break: None,
        }
    }

    pub fn tick(&mut self) -> Result<Option<String>> {
        let mut notification = None;

        if self.adapter.is_none() {
            if let Some(game) = detect_running_game() {
                self.adapter = Some(create_adapter(game));
            }
        }

        if self.session_id.is_some() && Instant::now() > self.abandon_deadline {
            notification = Some(self.finalize_session_message());
            self.reset_session_state();
            self.adapter = None;
            return Ok(notification);
        }

        if self.session_id.is_some()
            && !self.stint_gap_open
            && Instant::now() > self.heartbeat_deadline
        {
            let pit_aware = self
                .adapter
                .as_ref()
                .is_some_and(|a| a.detects_pit_stints());
            notification = self.open_stint_gap(pit_aware);
        }

        let event = {
            let Some(adapter) = self.adapter.as_mut() else {
                return Ok(None);
            };
            adapter.poll()
        };
        match event {
            AdapterEvent::Disconnected => {
                if self.session_id.is_some() {
                    notification = Some(self.finalize_session_message());
                }
                self.reset_session_state();
                self.adapter = None;
            }
            AdapterEvent::SessionInfo(info) => {
                notification = self.handle_session_info(info)?;
            }
            AdapterEvent::LapStarted { lap_number } => {
                self.mark_live_physics();
                // A fresh lap boundary after a resume means the buffer is whole
                // again; the truncated-lap taint no longer applies.
                self.stint_gap_during_lap = false;
                self.current_lap_number = lap_number;
                self.current_lap_samples.clear();
            }
            AdapterEvent::LapCompleted(mut summary) => {
                self.mark_live_physics();
                if std::mem::take(&mut self.stint_gap_during_lap) {
                    // Telemetry for this lap was truncated by a >10 s physics
                    // freeze — the trace is unusable, so don't trust the lap.
                    // `flush_lap` throws out a fragmentary trace on its own
                    // measure; this still earns its keep for the freeze that
                    // landed early enough to leave most of the lap recorded
                    // and the sim stopped dead all the same.
                    summary.valid = false;
                }
                if let Some(session_id) = self.session_id {
                    if let Some(flushed) = self.flush_lap(session_id, &summary)? {
                        notification = Some(format!(
                            "Lap {} saved — {} ({})",
                            summary.lap_number,
                            format_lap_time(summary.lap_time_ms),
                            lap_state_label(summary.valid, flushed.trace_is_whole())
                        ));
                    }
                }
                self.current_lap_samples.clear();
                self.current_lap_number = summary.lap_number + 1;
            }
            AdapterEvent::StintBoundary => {
                // The car came back out of the pits / garage. Adapters that emit
                // this are pit-aware, so a concurrent freeze gap never advanced
                // the stint itself — this event is what rolls it. `mark_live_physics`
                // also lifts any freeze pause.
                self.mark_live_physics();
                notification = self.begin_pit_stint();
            }
            AdapterEvent::Telemetry(sample) => {
                self.mark_live_physics();
                if !self.paused {
                    self.current_lap_samples.push(sample);
                    self.samples_recorded += 1;
                    let elapsed = self.last_tick.elapsed().as_secs_f32();
                    if elapsed > 0.0 {
                        self.sample_rate_estimate = 1.0 / elapsed;
                    }
                    self.last_tick = Instant::now();
                }
            }
            AdapterEvent::Heartbeat | AdapterEvent::None => {}
        }

        Ok(notification)
    }

    fn handle_session_info(&mut self, info: SessionInfo) -> Result<Option<String>> {
        if self.session_id.is_none() {
            return self.start_session(info);
        }

        let boundary_changed = self.session_info.as_ref().is_some_and(|current| {
            session_track_changed(current, &info) || session_car_changed(current, &info)
        });

        if boundary_changed {
            let _ = self.finalize_session_message();
            self.reset_session_state();
            return self.start_session(info);
        }

        // Same track and car, new weekend phase (Practice → Qualifying → Race):
        // keep the one recording and roll onto a fresh, phase-labelled stint.
        let phase_changed = self
            .session_info
            .as_ref()
            .is_some_and(|current| session_kind_changed(current, &info));
        if phase_changed {
            let kind = info.session_kind;
            // Remember the phase we just moved into. Comparing the *next*
            // announce against a stale kind swallows it — Q → R read against a
            // still-stored R looks like no change at all, and the race then
            // shares the qualifying stint (with the game's lap numbers
            // restarting inside it).
            let no_laps_yet = self.current_stint == 1 && !self.laps_in_current_stint;
            self.session_info = Some(info);
            if no_laps_yet {
                // Nothing recorded under the entry kind — it was a load-time
                // reading, not a phase the session ever ran. Re-label the row.
                if let Some(session_id) = self.session_id {
                    let session = self.session_info.as_ref().expect("just stored");
                    self.db.lock().update_session_metadata(
                        session_id,
                        &session.track_id,
                        &session.track,
                        &session.car,
                        &session.game_version,
                        &session.player_name,
                        kind,
                    )?;
                }
            }
            return Ok(self.begin_phase_stint(kind));
        }

        if let Some(session_id) = self.session_id {
            let current = self.session_info.as_ref();
            let current_kind = current.map_or(SessionKind::Unknown, |c| c.session_kind);
            let needs_update = current.is_none_or(|current| {
                (current.track.trim().is_empty() && !info.track.trim().is_empty())
                    || (current_kind == SessionKind::Unknown
                        && info.session_kind != SessionKind::Unknown)
            });
            if needs_update {
                // Never regress a known kind to Unknown on a plain metadata fill-in.
                let session_kind = match info.session_kind {
                    SessionKind::Unknown => current_kind,
                    known => known,
                };
                // The sim only just named the phase the current stint has been
                // running in — label its laps too.
                if current_kind == SessionKind::Unknown
                    && session_kind != SessionKind::Unknown
                {
                    self.current_stint_kind = session_kind;
                }
                self.db.lock().update_session_metadata(
                    session_id,
                    &info.track_id,
                    &info.track,
                    &info.car,
                    &info.game_version,
                    &info.player_name,
                    session_kind,
                )?;
                self.session_info = Some(SessionInfo {
                    session_kind,
                    ..info
                });
            }
        }

        Ok(None)
    }

    fn start_session(&mut self, info: SessionInfo) -> Result<Option<String>> {
        if info.track.trim().is_empty() || info.car.trim().is_empty() {
            return Ok(None);
        }
        let id = self.db.lock().create_session_with_kind(
            info.game,
            &info.track_id,
            &info.track,
            &info.car,
            &info.game_version,
            &info.player_name,
            info.session_kind,
        )?;
        self.session_id = Some(id);
        self.current_stint_kind = info.session_kind;
        self.session_info = Some(info);
        self.paused = false;
        self.samples_recorded = 0;
        self.current_stint = 1;
        self.laps_in_current_stint = false;
        self.stint_gap_open = false;
        self.stint_gap_during_lap = false;
        self.stint_break_armed = false;
        self.pending_stint_break = None;
        self.current_lap_samples.clear();
        let now = Instant::now();
        self.heartbeat_deadline = now + LIVE_PHYSICS_TIMEOUT;
        self.abandon_deadline = now + SESSION_ABANDON_TIMEOUT;
        self.last_live_physics = now;
        let session = self.session_info.as_ref().unwrap();
        Ok(Some(format!(
            "Recording {} — {} / {}",
            session.game.short_label(),
            session.track,
            session.car
        )))
    }

    fn mark_live_physics(&mut self) {
        let now = Instant::now();
        if self.stint_gap_open && self.stint_break_armed {
            // Resuming from a gap that split the stint — the freeze lasted from
            // the last live sample until now.
            let secs = self.last_live_physics.elapsed().as_secs();
            self.pending_stint_break = Some(secs.min(u32::MAX as u64) as u32);
            self.stint_break_armed = false;
        }
        self.stint_gap_open = false;
        self.heartbeat_deadline = now + LIVE_PHYSICS_TIMEOUT;
        self.abandon_deadline = now + SESSION_ABANDON_TIMEOUT;
        self.last_live_physics = now;
    }

    /// Roll onto the next stint if the current one has persisted laps to
    /// separate from. `arm_break` measures the resume gap as a break length
    /// (a real physics freeze); a clean phase change passes `false`. Returns
    /// whether the stint number advanced.
    fn advance_stint(&mut self, arm_break: bool) -> bool {
        if !self.laps_in_current_stint {
            return false;
        }
        self.current_stint += 1;
        self.laps_in_current_stint = false;
        self.stint_break_armed = arm_break;
        true
    }

    /// A lap in progress when the stint boundary lands has a truncated trace and
    /// can't be trusted as a clean lap.
    fn discard_in_progress_lap(&mut self) {
        if !self.current_lap_samples.is_empty() {
            self.stint_gap_during_lap = true;
        }
        self.current_lap_samples.clear();
    }

    /// A live-physics freeze reached `LIVE_PHYSICS_TIMEOUT`. Always taints an
    /// in-progress lap (its trace has a hole) and pauses the heartbeat until the
    /// sim resumes. Whether it also *splits the stint* depends on the game:
    /// when the adapter has real pit/garage detection (`pit_aware`), a bare
    /// freeze — alt-tab, pause menu, sim hitch — is not a new stint and the
    /// `StintBoundary` event is the only thing that rolls one.
    ///
    /// Returns a notification only when the freeze itself splits the stint.
    fn open_stint_gap(&mut self, pit_aware: bool) -> Option<String> {
        let notification = if pit_aware {
            None
        } else {
            self.advance_stint(true)
                .then(|| format!("Stint {} — break detected", self.current_stint))
        };
        self.discard_in_progress_lap();
        self.stint_gap_open = true;
        self.heartbeat_deadline = far_future();
        notification
    }

    /// The sim reported the car cycling out through the pit lane / garage
    /// (return-to-garage, or a normal pit stop) after at least one timed lap.
    /// Roll onto a fresh stint with no break time. No-op until the stint has a
    /// persisted lap to separate from (`advance_stint`).
    fn begin_pit_stint(&mut self) -> Option<String> {
        let bumped = self.advance_stint(false);
        self.discard_in_progress_lap();
        bumped.then(|| format!("Stint {} — out of the pits", self.current_stint))
    }

    /// Handle a weekend phase change (Practice → Qualifying → Race) on the same
    /// track and car: stay in the one recording, but start a fresh stint tagged
    /// with the new phase.
    ///
    /// The bump runs whether or not a freeze gap is open. Loading the next phase
    /// always freezes physics well past `LIVE_PHYSICS_TIMEOUT`, and for a
    /// pit-aware adapter that gap no longer advances the stint by itself — so
    /// skipping the bump here left the whole weekend on one stint. When the gap
    /// *did* advance it (non-pit-aware games), `advance_stint` is a no-op on the
    /// fresh stint and its measured break survives.
    fn begin_phase_stint(&mut self, kind: SessionKind) -> Option<String> {
        self.advance_stint(false);
        self.discard_in_progress_lap();
        self.current_stint_kind = kind;
        Some(format!(
            "{} — recording continues (stint {})",
            kind.label(),
            self.current_stint
        ))
    }

    /// Returns what was persisted, or `None` when the lap was skipped.
    fn flush_lap(
        &mut self,
        session_id: Uuid,
        summary: &sim_core::LapSummary,
    ) -> Result<Option<FlushedLap>> {
        let mut summary = summary.clone();
        if summary.fuel_used_l.is_none() {
            summary.fuel_used_l = compute_fuel_used_l(&self.current_lap_samples);
        }
        let grid = resample_to_distance_grid(&self.current_lap_samples);
        if grid.is_empty() || summary.lap_time_ms == 0 {
            tracing::warn!(
                lap = summary.lap_number,
                samples = self.current_lap_samples.len(),
                lap_time_ms = summary.lap_time_ms,
                "skipping lap with insufficient telemetry or zero time"
            );
            return Ok(None);
        }

        // Only the first persisted lap of a fresh stint carries the break marker.
        let stint_break_s = if self.laps_in_current_stint {
            None
        } else {
            self.pending_stint_break
        };

        // Measured from the raw samples rather than the grid: these carry every
        // position the sim reported, so this is the ground the car actually
        // covered. Zero means the trace held no usable positions — store nothing
        // and leave it to the read-time backfill to try again from the parquet.
        let driven_m = lap_distance_m(&self.current_lap_samples);

        // Measured from the raw samples for the same reason as the distance:
        // the grid closes a hole in the middle back up.
        //
        // A lap recorded in fragments is not a lap of this track — its trace is
        // stretched over ground it never covered, and its sectors and fuel
        // belong to whatever part of it was caught. Storing the measure is
        // enough to keep it out of the best lap, the leaderboard and the
        // session's averages: `sim_storage` reads those off `usable_lap_sql`,
        // which asks for a whole trace as well as a valid lap. `valid` itself
        // stays the sim's verdict, so a measure that ever reads a lap wrong
        // costs a label rather than the lap.
        let coverage = trace_coverage(&self.current_lap_samples, summary.lap_time_ms);

        let lap_id = Uuid::new_v4();
        let rel = format!("sessions/{session_id}/laps/{lap_id}.parquet");
        let abs = resolve_data_relative(&self.data_dir, &rel)?;
        // File I/O first (no DB lock), then short DB insert.
        write_lap_parquet(&abs, &grid)?;
        let manifest = channel_manifest_json(&grid);
        if let Err(err) = self.db.lock().insert_lap_with_id(
            lap_id,
            session_id,
            &summary,
            &rel,
            self.sample_rate_estimate,
            &manifest,
            self.current_stint,
            stint_break_s,
            self.current_stint_kind,
            (driven_m > 0.0).then_some(driven_m),
            coverage,
        ) {
            let _ = std::fs::remove_file(&abs);
            return Err(err);
        }
        self.laps_in_current_stint = true;
        self.pending_stint_break = None;
        Ok(Some(FlushedLap { coverage }))
    }

    fn finalize_session_message(&mut self) -> String {
        let laps = self
            .session_id
            .and_then(|id| self.db.lock().list_laps(id).ok())
            .map(|l| l.len())
            .unwrap_or(0);
        if let Some(id) = self.session_id.take() {
            let _ = self.db.lock().finalize_session(id);
        }
        format!("Session saved — {laps} laps")
    }

    fn reset_session_state(&mut self) {
        self.session_id = None;
        self.session_info = None;
        self.current_lap_samples.clear();
        self.current_lap_number = 1;
        self.paused = false;
        self.heartbeat_deadline = far_future();
        self.abandon_deadline = far_future();
        self.current_stint = 1;
        self.current_stint_kind = SessionKind::Unknown;
        self.laps_in_current_stint = false;
        self.stint_gap_open = false;
        self.stint_gap_during_lap = false;
        self.stint_break_armed = false;
        self.pending_stint_break = None;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if !paused {
            let now = Instant::now();
            self.heartbeat_deadline = now + Duration::from_secs(5);
            self.abandon_deadline = now + SESSION_ABANDON_TIMEOUT;
            self.stint_gap_open = false;
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn status(&self) -> RecordingStatus {
        RecordingStatus {
            active: self.adapter.is_some(),
            paused: self.paused,
            game: self.adapter.as_ref().map(|a| a.game_id()),
            track: self.session_info.as_ref().map(|s| s.track.clone()),
            current_lap: self.current_lap_number,
            samples_recorded: self.samples_recorded,
        }
    }
}

fn create_adapter(game: GameId) -> Box<dyn GameAdapter> {
    match game {
        GameId::Acc => Box::new(AccAdapter::new()),
        GameId::Ac => Box::new(AcAdapter::new()),
        GameId::Lmu => Box::new(LmuAdapter::new()),
        GameId::F1_25 => Box::new(F1Adapter::new()),
    }
}

/// A lap that went to disk, and what the recorder learned about its trace on
/// the way there.
struct FlushedLap {
    /// Fraction of the lap the recording covers, `None` when unmeasurable.
    coverage: Option<f32>,
}

impl FlushedLap {
    fn trace_is_whole(&self) -> bool {
        sim_core::trace_is_whole(self.coverage)
    }
}

/// How a saved lap is described in the notification. `valid` is what the sim
/// made of the lap; a lap recorded in fragments is thrown out whatever that
/// was, so it is worth saying which of the two happened.
fn lap_state_label(valid: bool, trace_is_whole: bool) -> &'static str {
    match (valid, trace_is_whole) {
        (_, false) => "unusable — partial trace",
        (true, true) => "valid",
        (false, true) => "invalid",
    }
}

fn format_lap_time(ms: u32) -> String {
    let minutes = ms / 60_000;
    let seconds = (ms % 60_000) as f32 / 1000.0;
    if minutes > 0 {
        format!("{minutes}:{seconds:06.3}")
    } else {
        format!("{seconds:.3}")
    }
}

#[cfg(test)]
impl RecordingService {
    /// Build a service with a caller-supplied adapter instead of one discovered
    /// through `detect_running_game`. Test-only seam for driving `tick()`.
    fn with_adapter(
        db: Arc<Mutex<Database>>,
        data_dir: PathBuf,
        adapter: Box<dyn GameAdapter>,
    ) -> Self {
        let mut svc = Self::new(db, data_dir);
        svc.adapter = Some(adapter);
        svc
    }

    fn expire_heartbeat(&mut self) {
        self.heartbeat_deadline = Instant::now() - Duration::from_secs(1);
    }

    fn expire_abandon(&mut self) {
        self.abandon_deadline = Instant::now() - Duration::from_secs(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sim_core::{LapSummary, SectorTimes};
    use std::collections::VecDeque;

    /// Yields one scripted event per `poll`, then `None` forever.
    struct FakeAdapter {
        events: VecDeque<AdapterEvent>,
        pit_aware: bool,
    }

    impl FakeAdapter {
        fn new(events: Vec<AdapterEvent>) -> Self {
            Self {
                events: events.into(),
                pit_aware: false,
            }
        }

        /// A pit-aware adapter: a physics-freeze gap no longer splits the stint;
        /// only a `StintBoundary` event does.
        fn new_pit_aware(events: Vec<AdapterEvent>) -> Self {
            Self {
                events: events.into(),
                pit_aware: true,
            }
        }
    }

    impl GameAdapter for FakeAdapter {
        fn game_id(&self) -> GameId {
            GameId::Acc
        }
        fn is_active(&self) -> bool {
            true
        }
        fn detects_pit_stints(&self) -> bool {
            self.pit_aware
        }
        fn poll(&mut self) -> AdapterEvent {
            self.events.pop_front().unwrap_or(AdapterEvent::None)
        }
    }

    fn session_info(track_id: &str, track: &str) -> SessionInfo {
        session_info_car(track_id, track, "Ferrari 296 GT3")
    }

    fn session_info_car(track_id: &str, track: &str, car: &str) -> SessionInfo {
        SessionInfo {
            game: GameId::Acc,
            track_id: track_id.to_string(),
            track: track.to_string(),
            car: car.to_string(),
            game_version: "1.0".to_string(),
            player_name: "Tester".to_string(),
            session_kind: SessionKind::Unknown,
        }
    }

    fn session_info_kind(track: &str, kind: SessionKind) -> SessionInfo {
        SessionInfo {
            session_kind: kind,
            ..session_info(track, track)
        }
    }

    fn sample(distance_m: f32) -> TelemetrySample {
        TelemetrySample {
            timestamp: Utc::now(),
            lap_time_s: distance_m / 60.0,
            distance_m,
            speed_mps: 60.0,
            throttle: 1.0,
            brake: 0.0,
            steering: 0.0,
            gear: 4,
            rpm: 7000.0,
            pos_x: distance_m,
            pos_y: 0.0,
            pos_z: 0.0,
            fuel: Some(50.0 - distance_m / 1000.0),
            tyre_temp_fl: None,
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
            raw: serde_json::Value::Null,
        }
    }

    fn telemetry(count: usize) -> Vec<AdapterEvent> {
        (0..count)
            .map(|i| AdapterEvent::Telemetry(sample(i as f32 * 50.0)))
            .collect()
    }

    fn lap_completed(lap_number: u32, valid: bool) -> AdapterEvent {
        AdapterEvent::LapCompleted(LapSummary {
            lap_number,
            lap_time_ms: 100_000 + lap_number,
            valid,
            sectors: SectorTimes {
                s1_ms: None,
                s2_ms: None,
                s3_ms: None,
            },
            tyre_compound: None,
            tc_level: None,
            abs_level: None,
            fuel_used_l: None,
        })
    }

    fn service(events: Vec<AdapterEvent>) -> (tempfile::TempDir, RecordingService) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("simtelemetry.db")).unwrap();
        let svc = RecordingService::with_adapter(
            Arc::new(Mutex::new(db)),
            dir.path().to_path_buf(),
            Box::new(FakeAdapter::new(events)),
        );
        (dir, svc)
    }

    fn service_pit_aware(events: Vec<AdapterEvent>) -> (tempfile::TempDir, RecordingService) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("simtelemetry.db")).unwrap();
        let svc = RecordingService::with_adapter(
            Arc::new(Mutex::new(db)),
            dir.path().to_path_buf(),
            Box::new(FakeAdapter::new_pit_aware(events)),
        );
        (dir, svc)
    }

    /// Drain every scripted event plus a trailing `None`.
    fn run(svc: &mut RecordingService) {
        for _ in 0..256 {
            let _ = svc.tick().unwrap();
        }
    }

    #[test]
    fn track_change_starts_new_session_and_resets_stint() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        events.push(AdapterEvent::SessionInfo(session_info("spa", "Spa")));
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let sessions = svc.db.lock().list_sessions().unwrap();
        assert_eq!(sessions.len(), 2, "track change should open a second session");
        for session in sessions {
            let laps = svc.db.lock().list_laps(session.id).unwrap();
            assert_eq!(laps.len(), 1);
            assert_eq!(laps[0].stint, 1, "each fresh session starts at stint 1");
        }
        drop(dir);
    }

    #[test]
    fn car_change_on_same_track_starts_new_session() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info_car(
            "monza", "Monza", "Ferrari 296 GT3",
        ))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        events.push(AdapterEvent::SessionInfo(session_info_car(
            "monza", "Monza", "Porsche 992 GT3 R",
        )));
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let sessions = svc.db.lock().list_sessions().unwrap();
        assert_eq!(sessions.len(), 2, "car swap should open a second session");
        let cars: Vec<_> = sessions.iter().map(|s| s.car.as_str()).collect();
        assert!(cars.contains(&"Ferrari 296 GT3"));
        assert!(cars.contains(&"Porsche 992 GT3 R"));
        for session in &sessions {
            assert_eq!(svc.db.lock().list_laps(session.id).unwrap()[0].stint, 1);
        }
        assert_eq!(svc.current_stint, 1);
        drop(dir);
    }

    #[test]
    fn weekend_phases_stay_one_session_split_into_labelled_stints() {
        // Practice → Qualifying → Race on the same track/car: one recording,
        // each phase its own stint carrying the phase label.
        let mut events = vec![AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Practice,
        ))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        events.push(AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Qualifying,
        )));
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        events.push(AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Race,
        )));
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let sessions = svc.db.lock().list_sessions().unwrap();
        assert_eq!(sessions.len(), 1, "the weekend is one recording");
        let laps = svc.db.lock().list_laps(sessions[0].id).unwrap();
        assert_eq!(laps.len(), 3);
        assert_eq!(
            laps.iter().map(|l| l.stint).collect::<Vec<_>>(),
            vec![1, 2, 3],
            "each phase gets its own stint"
        );
        assert_eq!(
            laps.iter().map(|l| l.stint_kind).collect::<Vec<_>>(),
            vec![
                Some(SessionKind::Practice),
                Some(SessionKind::Qualifying),
                Some(SessionKind::Race),
            ],
            "each stint carries its phase label"
        );
        drop(dir);
    }

    #[test]
    fn phase_change_after_the_load_freeze_still_splits_the_stint() {
        // Loading the next phase always freezes physics past the timeout, and
        // for a pit-aware game that freeze deliberately does not split. The
        // phase change itself must still roll the stint.
        let mut events = vec![AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Qualifying,
        ))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service_pit_aware(events);
        run(&mut svc);

        svc.expire_heartbeat();
        let _ = svc.tick().unwrap();
        assert!(svc.stint_gap_open, "the phase load froze physics");

        let mut race = vec![AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Race,
        ))];
        race.extend(telemetry(6));
        race.push(lap_completed(1, true));
        svc.adapter = Some(Box::new(FakeAdapter::new_pit_aware(race)));
        run(&mut svc);

        let sessions = svc.db.lock().list_sessions().unwrap();
        assert_eq!(sessions.len(), 1, "the weekend is one recording");
        let laps = svc.db.lock().list_laps(sessions[0].id).unwrap();
        assert_eq!(
            laps.iter().map(|l| l.stint).collect::<Vec<_>>(),
            vec![1, 2],
            "the race laps belong to their own stint"
        );
        assert_eq!(
            laps.iter().map(|l| l.stint_kind).collect::<Vec<_>>(),
            vec![Some(SessionKind::Qualifying), Some(SessionKind::Race)],
        );
        drop(dir);
    }

    #[test]
    fn phase_change_stores_the_new_kind_so_the_next_one_is_seen() {
        // ACC can report the weekend's last phase for the first announce, before
        // the session the driver is actually in settles. Holding that stale kind
        // made the real Q → R change compare equal and vanish, dropping the race
        // into the qualifying stint with the game's lap numbers restarting.
        let events = vec![AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Race,
        ))];
        let (dir, mut svc) = service_pit_aware(events);
        run(&mut svc);

        let mut quali = vec![AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Qualifying,
        ))];
        quali.extend(telemetry(6));
        quali.push(lap_completed(1, true));
        svc.adapter = Some(Box::new(FakeAdapter::new_pit_aware(quali)));
        run(&mut svc);
        assert_eq!(svc.current_stint, 1, "nothing recorded yet to split from");

        let mut race = vec![AdapterEvent::SessionInfo(session_info_kind(
            "monza",
            SessionKind::Race,
        ))];
        race.extend(telemetry(6));
        race.push(lap_completed(1, true));
        svc.adapter = Some(Box::new(FakeAdapter::new_pit_aware(race)));
        run(&mut svc);

        let sessions = svc.db.lock().list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].session_kind,
            SessionKind::Qualifying,
            "the entry kind is re-labelled while the session has no laps"
        );
        let laps = svc.db.lock().list_laps(sessions[0].id).unwrap();
        assert_eq!(
            laps.iter().map(|l| (l.stint, l.stint_kind)).collect::<Vec<_>>(),
            vec![
                (1, Some(SessionKind::Qualifying)),
                (2, Some(SessionKind::Race)),
            ],
            "the second phase change is not swallowed by the stale kind"
        );
        drop(dir);
    }

    #[test]
    fn unknown_session_kind_does_not_split_or_bump_stint() {
        // Sims that never report a session type: SessionInfo re-announces are
        // plain metadata, no new session and no new stint.
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        events.push(AdapterEvent::SessionInfo(session_info("monza", "Monza")));
        events.extend(telemetry(6));
        events.push(lap_completed(2, true));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let sessions = svc.db.lock().list_sessions().unwrap();
        assert_eq!(sessions.len(), 1, "no session type means no split");
        let laps = svc.db.lock().list_laps(sessions[0].id).unwrap();
        assert_eq!(laps.len(), 2);
        assert!(laps.iter().all(|l| l.stint == 1), "one stint");
        assert!(laps.iter().all(|l| l.stint_kind.is_none()));
        drop(dir);
    }

    #[test]
    fn stint_bumps_after_gap_with_recorded_laps() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(whole_lap());
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);
        run(&mut svc);
        assert_eq!(svc.current_stint, 1);

        // Physics freezes for >10 s, then the driver returns and completes a lap.
        svc.expire_heartbeat();
        let note = svc.tick().unwrap(); // gap check runs before the (empty) poll
        assert_eq!(svc.current_stint, 2, "a gap after real laps opens a new stint");
        assert_eq!(note.as_deref(), Some("Stint 2 — break detected"));

        let mut resume = whole_lap();
        resume.push(lap_completed(2, true));
        resume.extend(whole_lap());
        resume.push(lap_completed(3, true));
        svc.adapter = Some(Box::new(FakeAdapter::new(resume)));
        run(&mut svc);

        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps.len(), 3);
        assert_eq!(laps[0].stint, 1);
        assert_eq!(laps[0].stint_break_s, None);
        assert_eq!(laps[1].stint, 2, "lap after the gap belongs to stint 2");
        assert!(laps[1].valid, "a whole lap after the gap stays valid");
        assert!(
            laps[1].stint_break_s.is_some(),
            "first lap of the new stint records the break length"
        );
        assert_eq!(
            laps[2].stint_break_s, None,
            "only the first lap of the stint carries the break marker"
        );
        drop(dir);
    }

    #[test]
    fn pit_out_boundary_opens_a_break_less_stint() {
        // Return-to-garage: adapter emits StintBoundary, no physics freeze.
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(whole_lap());
        events.push(lap_completed(1, true));
        events.push(AdapterEvent::StintBoundary);
        events.extend(whole_lap());
        events.push(lap_completed(2, true));
        let (dir, mut svc) = service_pit_aware(events);

        run(&mut svc);

        assert_eq!(svc.current_stint, 2);
        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps.len(), 2);
        assert_eq!(laps[0].stint, 1);
        assert_eq!(laps[1].stint, 2, "lap after the pit-out belongs to stint 2");
        assert_eq!(
            laps[1].stint_break_s, None,
            "a pit-out with live physics records no break length"
        );
        assert!(laps[1].valid, "the flyer after the out-lap stays valid");
        drop(dir);
    }

    #[test]
    fn pit_out_boundary_without_laps_does_not_bump_stint() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(3)); // out-lap in progress, no completed lap yet
        events.push(AdapterEvent::StintBoundary);
        let (dir, mut svc) = service_pit_aware(events);

        run(&mut svc);

        assert_eq!(
            svc.current_stint, 1,
            "no timed lap in the stint yet, so nothing to split"
        );
        drop(dir);
    }

    #[test]
    fn pit_aware_adapter_does_not_split_on_a_bare_freeze() {
        // A pit-aware game: alt-tab / pause / sim hitch freezes physics past the
        // timeout but the car never cycled the pits — no new stint.
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service_pit_aware(events);
        run(&mut svc);
        assert_eq!(svc.current_stint, 1);

        svc.expire_heartbeat();
        let note = svc.tick().unwrap();
        assert_eq!(svc.current_stint, 1, "a bare freeze is not a stint boundary");
        assert_eq!(note, None);
        assert!(svc.stint_gap_open, "the freeze still pauses the heartbeat");

        // Resume with more laps — still stint 1.
        let mut resume = telemetry(6);
        resume.push(lap_completed(2, true));
        svc.adapter = Some(Box::new(FakeAdapter::new_pit_aware(resume)));
        run(&mut svc);

        assert_eq!(svc.current_stint, 1);
        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps.len(), 2);
        assert!(laps.iter().all(|l| l.stint == 1), "one continuous stint");
        assert!(laps.iter().all(|l| l.stint_break_s.is_none()));
        drop(dir);
    }

    #[test]
    fn pit_aware_freeze_then_pit_out_splits_once() {
        // RTG that also froze physics on the garage load: the freeze doesn't
        // split, the following StintBoundary does — exactly one bump.
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service_pit_aware(events);
        run(&mut svc);

        svc.expire_heartbeat();
        let _ = svc.tick().unwrap();
        assert_eq!(svc.current_stint, 1, "freeze alone doesn't split");

        let mut resume = vec![AdapterEvent::StintBoundary];
        resume.extend(telemetry(6));
        resume.push(lap_completed(2, true));
        svc.adapter = Some(Box::new(FakeAdapter::new_pit_aware(resume)));
        run(&mut svc);

        assert_eq!(svc.current_stint, 2, "the pit-out rolls the stint, once");
        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps[1].stint, 2);
        assert_eq!(
            laps[1].stint_break_s, None,
            "pit-out stints carry no break length"
        );
        drop(dir);
    }

    #[test]
    fn gap_without_laps_does_not_bump_stint() {
        let (dir, mut svc) = service(vec![AdapterEvent::SessionInfo(session_info(
            "monza", "Monza",
        ))]);
        run(&mut svc);
        assert_eq!(svc.current_stint, 1);

        svc.expire_heartbeat();
        let note = svc.tick().unwrap();
        assert_eq!(
            svc.current_stint, 1,
            "no laps in the stint yet, so nothing to split"
        );
        assert!(svc.stint_gap_open);
        assert_eq!(note, None, "a gap that splits nothing is silent");
        drop(dir);
    }

    #[test]
    fn mid_lap_gap_invalidates_the_completed_lap_only() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(3)); // lap in progress, buffer non-empty
        let (dir, mut svc) = service(events);
        run(&mut svc);
        assert!(!svc.current_lap_samples.is_empty());

        svc.expire_heartbeat();
        let _ = svc.tick().unwrap();
        assert!(svc.stint_gap_during_lap, "mid-lap freeze taints the lap");

        // Resume: finish the truncated lap, then run a clean one.
        let mut resume = telemetry(6);
        resume.push(lap_completed(1, true));
        resume.extend(whole_lap());
        resume.push(lap_completed(2, true));
        svc.adapter = Some(Box::new(FakeAdapter::new(resume)));
        run(&mut svc);

        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps.len(), 2);
        assert!(!laps[0].valid, "the lap straddling the freeze is invalid");
        assert!(laps[1].valid, "the next clean lap is unaffected");
        drop(dir);
    }

    #[test]
    fn a_lap_records_the_ground_it_covered() {
        // `telemetry` walks pos_x in 50 m steps, so ten samples drove 450 m.
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(10));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps.len(), 1);
        let measured = laps[0].lap_distance_m.expect("the lap is measured");
        assert!((measured - 450.0).abs() < 1.0, "got {measured}");
        drop(dir);
    }

    #[test]
    fn a_short_lap_measures_shorter_than_a_full_one() {
        // The Nurburgring 24h case in miniature: two laps to the same
        // start/finish line, the second round a fraction of the route. Their
        // times say nothing about that; the ground they covered does.
        let mut events = vec![AdapterEvent::SessionInfo(session_info(
            "nurburgring_24h",
            "Nurburgring 24h",
        ))];
        events.extend(telemetry(20));
        events.push(lap_completed(1, true));
        events.extend(telemetry(5));
        events.push(lap_completed(2, false));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps.len(), 2);
        let full = laps[0].lap_distance_m.unwrap();
        let short = laps[1].lap_distance_m.unwrap();
        assert!(short < full * 0.5, "short {short} against full {full}");
        drop(dir);
    }

    /// Telemetry covering `from`..`to` of a lap at the helper's 50 m spacing,
    /// which is 0.83 s per sample against `lap_completed`'s 100 s lap: 120
    /// samples is a whole lap, and starting late is a recording that attached
    /// part-way round.
    fn telemetry_range(from: usize, to: usize) -> Vec<AdapterEvent> {
        (from..to)
            .map(|i| AdapterEvent::Telemetry(sample(i as f32 * 50.0)))
            .collect()
    }

    /// A recording of a whole lap: `lap_completed` times the lap at 100 s and
    /// the samples step 0.83 s each, so this covers the lap end to end and the
    /// lap is usable. Tests that assert a lap stays valid need one — a lap the
    /// recorder only caught part of is thrown out on its coverage alone.
    fn whole_lap() -> Vec<AdapterEvent> {
        telemetry_range(0, 121)
    }

    #[test]
    fn a_cut_lap_is_invalid_with_a_whole_trace() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry_range(0, 120));
        events.push(lap_completed(1, false));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        let coverage = laps[0].trace_coverage.expect("the trace is measured");
        assert!(!laps[0].valid, "the sim scored the lap invalid");
        assert!(
            coverage >= sim_core::COMPLETE_TRACE_COVERAGE,
            "a cut lap is still recorded whole, got {coverage}"
        );
        drop(dir);
    }

    #[test]
    fn a_recording_that_attached_mid_lap_does_not_count() {
        // The sim scored the lap and its time is real, but half of it was never
        // recorded: there is no trace of this lap round the track to show. The
        // sim's verdict is kept as it was and the lap stops counting on the
        // measure — it is not the session's best lap however quick it reads.
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry_range(60, 120));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);

        run(&mut svc);

        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        let coverage = laps[0].trace_coverage.expect("the trace is measured");
        assert!((coverage - 0.5).abs() < 0.02, "got {coverage}");
        assert!(laps[0].valid, "the sim's verdict is left alone");
        assert!(
            !laps[0].is_best,
            "but a lap recorded in part is not the session's best lap"
        );
        let sessions = svc.db.lock().list_sessions().unwrap();
        assert_eq!(
            sessions[0].best_lap_time_ms, None,
            "and the session has no best lap to show"
        );
        drop(dir);
    }

    #[test]
    fn a_lap_truncated_by_a_freeze_is_partial_as_well_as_invalid() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry_range(0, 30));
        let (dir, mut svc) = service(events);
        run(&mut svc);

        svc.expire_heartbeat();
        let _ = svc.tick().unwrap();

        // Resume half a lap later and finish the lap: the freeze cleared the
        // buffer, so what is stored starts where the sim came back.
        let mut resume = telemetry_range(90, 120);
        resume.push(lap_completed(1, true));
        svc.adapter = Some(Box::new(FakeAdapter::new(resume)));
        run(&mut svc);

        let session = svc.db.lock().list_sessions().unwrap()[0].id;
        let laps = svc.db.lock().list_laps(session).unwrap();
        let coverage = laps[0].trace_coverage.expect("the trace is measured");
        assert!(!laps[0].valid, "the freeze taints the lap itself");
        assert!(
            coverage < sim_core::COMPLETE_TRACE_COVERAGE,
            "and the trace it left is a fragment, got {coverage}"
        );
        drop(dir);
    }

    #[test]
    fn idle_past_abandon_timeout_finalizes_session() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);
        run(&mut svc);
        let session = svc.db.lock().list_sessions().unwrap()[0].id;

        svc.expire_abandon();
        let note = svc.tick().unwrap();

        assert!(note.unwrap_or_default().starts_with("Session saved"));
        assert!(svc.session_id.is_none(), "session is finalized");
        assert!(!svc.status().active, "adapter dropped so detection restarts");
        let laps = svc.db.lock().list_laps(session).unwrap();
        assert_eq!(laps.len(), 1, "finalize keeps the recorded lap");
        drop(dir);
    }
}
