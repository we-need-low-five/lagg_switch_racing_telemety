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
    current_stint: u32,
    laps_in_current_stint: bool,
    stint_gap_open: bool,
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
            current_stint: 1,
            laps_in_current_stint: false,
            stint_gap_open: false,
        }
    }

    pub fn tick(&mut self) -> Result<Option<String>> {
        let mut notification = None;

        if self.adapter.is_none() {
            if let Some(game) = detect_running_game() {
                self.adapter = Some(create_adapter(game));
            }
        }

        if self.session_id.is_some()
            && !self.stint_gap_open
            && Instant::now() > self.heartbeat_deadline
        {
            self.open_stint_gap();
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
                self.current_lap_number = lap_number;
                self.current_lap_samples.clear();
            }
            AdapterEvent::LapCompleted(summary) => {
                self.mark_live_physics();
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
        self.current_lap_samples.clear();
        self.heartbeat_deadline = Instant::now() + LIVE_PHYSICS_TIMEOUT;
        let session = self.session_info.as_ref().unwrap();
        Ok(Some(format!(
            "Recording {} — {} / {}",
            session.game.short_label(),
            session.track,
            session.car
        )))
    }

    fn mark_live_physics(&mut self) {
        self.stint_gap_open = false;
        self.heartbeat_deadline = Instant::now() + LIVE_PHYSICS_TIMEOUT;
    }

    fn open_stint_gap(&mut self) {
        if self.laps_in_current_stint {
            self.current_stint += 1;
            self.laps_in_current_stint = false;
        }
        self.current_lap_samples.clear();
        self.stint_gap_open = true;
        self.heartbeat_deadline = far_future();
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
        ) {
            let _ = std::fs::remove_file(&abs);
            return Err(err);
        }
        self.laps_in_current_stint = true;
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
        self.current_stint = 1;
        self.laps_in_current_stint = false;
        self.stint_gap_open = false;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if !paused {
            self.heartbeat_deadline = Instant::now() + Duration::from_secs(5);
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
