use anyhow::Result;
use parking_lot::Mutex;
use sim_capture_acc::AccAdapter;
use sim_capture_ac::AcAdapter;
use sim_capture_f1::F1Adapter;
use sim_capture_lmu::LmuAdapter;
use sim_core::{
    channel_manifest_json, compute_fuel_used_l, resample_to_distance_grid, session_track_changed,
    AdapterEvent, GameAdapter, GameId, RecordingStatus, SessionInfo, TelemetrySample,
};
use sim_storage::{resolve_data_relative, write_lap_parquet, Database};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::detect_running_game;

const LIVE_PHYSICS_TIMEOUT: Duration = Duration::from_secs(30);

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
            notification = self.open_stint_gap();
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
                    // Telemetry for this lap was truncated by a >30 s physics
                    // freeze — the trace is unusable, so don't trust the lap.
                    summary.valid = false;
                }
                if let Some(session_id) = self.session_id {
                    if self.flush_lap(session_id, &summary)? {
                        notification = Some(format!(
                            "Lap {} saved — {} ({})",
                            summary.lap_number,
                            format_lap_time(summary.lap_time_ms),
                            if summary.valid { "valid" } else { "invalid" }
                        ));
                    }
                }
                self.current_lap_samples.clear();
                self.current_lap_number = summary.lap_number + 1;
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

        let track_changed = self
            .session_info
            .as_ref()
            .is_some_and(|current| session_track_changed(current, &info));

        if track_changed {
            let _ = self.finalize_session_message();
            self.reset_session_state();
            return self.start_session(info);
        }

        if let Some(session_id) = self.session_id {
            let current = self.session_info.as_ref();
            let needs_update = current.is_none_or(|current| {
                current.track.trim().is_empty() && !info.track.trim().is_empty()
            });
            if needs_update {
                self.db.lock().update_session_metadata(
                    session_id,
                    &info.track_id,
                    &info.track,
                    &info.car,
                    &info.game_version,
                    &info.player_name,
                )?;
                self.session_info = Some(info);
            }
        }

        Ok(None)
    }

    fn start_session(&mut self, info: SessionInfo) -> Result<Option<String>> {
        if info.track.trim().is_empty() || info.car.trim().is_empty() {
            return Ok(None);
        }
        let id = self.db.lock().create_session(
            info.game,
            &info.track_id,
            &info.track,
            &info.car,
            &info.game_version,
            &info.player_name,
        )?;
        self.session_id = Some(id);
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

    /// Returns a notification when the gap actually splits the stint.
    fn open_stint_gap(&mut self) -> Option<String> {
        let mut notification = None;
        if self.laps_in_current_stint {
            self.current_stint += 1;
            self.laps_in_current_stint = false;
            self.stint_break_armed = true;
            notification = Some(format!("Stint {} — break detected", self.current_stint));
        }
        if !self.current_lap_samples.is_empty() {
            // The freeze interrupted a lap already in progress. Its telemetry
            // trace is now truncated, so the lap it completes into is not a
            // clean lap — flag it so flush_lap marks it invalid.
            self.stint_gap_during_lap = true;
        }
        self.current_lap_samples.clear();
        self.stint_gap_open = true;
        self.heartbeat_deadline = far_future();
        notification
    }

    /// Returns true when a lap was persisted.
    fn flush_lap(
        &mut self,
        session_id: Uuid,
        summary: &sim_core::LapSummary,
    ) -> Result<bool> {
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
            return Ok(false);
        }

        // Only the first persisted lap of a fresh stint carries the break marker.
        let stint_break_s = if self.laps_in_current_stint {
            None
        } else {
            self.pending_stint_break
        };

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
        ) {
            let _ = std::fs::remove_file(&abs);
            return Err(err);
        }
        self.laps_in_current_stint = true;
        self.pending_stint_break = None;
        Ok(true)
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
    }

    impl FakeAdapter {
        fn new(events: Vec<AdapterEvent>) -> Self {
            Self {
                events: events.into(),
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
        fn poll(&mut self) -> AdapterEvent {
            self.events.pop_front().unwrap_or(AdapterEvent::None)
        }
    }

    fn session_info(track_id: &str, track: &str) -> SessionInfo {
        SessionInfo {
            game: GameId::Acc,
            track_id: track_id.to_string(),
            track: track.to_string(),
            car: "Ferrari 296 GT3".to_string(),
            game_version: "1.0".to_string(),
            player_name: "Tester".to_string(),
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
    fn stint_bumps_after_gap_with_recorded_laps() {
        let mut events = vec![AdapterEvent::SessionInfo(session_info("monza", "Monza"))];
        events.extend(telemetry(6));
        events.push(lap_completed(1, true));
        let (dir, mut svc) = service(events);
        run(&mut svc);
        assert_eq!(svc.current_stint, 1);

        // Physics freezes for >30 s, then the driver returns and completes a lap.
        svc.expire_heartbeat();
        let note = svc.tick().unwrap(); // gap check runs before the (empty) poll
        assert_eq!(svc.current_stint, 2, "a gap after real laps opens a new stint");
        assert_eq!(note.as_deref(), Some("Stint 2 — break detected"));

        let mut resume = telemetry(6);
        resume.push(lap_completed(2, true));
        resume.extend(telemetry(6));
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
        resume.extend(telemetry(6));
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
