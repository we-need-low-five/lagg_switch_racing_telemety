use anyhow::Result;
use parking_lot::Mutex;
use sim_capture_acc::AccAdapter;
use sim_capture_ac::AcAdapter;
use sim_capture_f1::F1Adapter;
use sim_capture_lmu::LmuAdapter;
use sim_core::{
    channel_manifest_json, compute_fuel_used_l, resample_to_distance_grid, AdapterEvent,
    GameAdapter, GameId, RecordingStatus, SessionInfo, TelemetrySample,
};
use sim_storage::{resolve_data_relative, write_lap_parquet, Database};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::detect_running_game;

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
            // Far future until a session starts.
            heartbeat_deadline: Instant::now() + Duration::from_secs(365 * 24 * 3600),
        }
    }

    pub fn tick(&mut self) -> Result<Option<String>> {
        let mut notification = None;

        if self.adapter.is_none() {
            if let Some(game) = detect_running_game() {
                self.adapter = Some(create_adapter(game));
            }
        }

        let Some(adapter) = self.adapter.as_mut() else {
            return Ok(None);
        };

        // Silent game / frozen SHM: finalize like disconnect.
        if self.session_id.is_some() && Instant::now() > self.heartbeat_deadline {
            notification = Some(self.finalize_session_message());
            self.reset_session_state();
            self.adapter = None;
            return Ok(notification);
        }

        let event = adapter.poll();
        match event {
            AdapterEvent::Disconnected => {
                if self.session_id.is_some() {
                    notification = Some(self.finalize_session_message());
                }
                self.reset_session_state();
                self.adapter = None;
            }
            AdapterEvent::SessionInfo(info) => {
                if self.session_id.is_none() {
                    if info.track.trim().is_empty() || info.car.trim().is_empty() {
                        return Ok(notification);
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
                    self.heartbeat_deadline = Instant::now() + Duration::from_secs(30);
                    notification = Some(format!(
                        "Recording {} — {} / {}",
                        adapter.game_id().short_label(),
                        self.session_info.as_ref().unwrap().track,
                        self.session_info.as_ref().unwrap().car
                    ));
                } else if let Some(session_id) = self.session_id {
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
            }
            AdapterEvent::LapStarted { lap_number } => {
                self.current_lap_number = lap_number;
                self.current_lap_samples.clear();
                self.heartbeat_deadline = Instant::now() + Duration::from_secs(30);
            }
            AdapterEvent::LapCompleted(summary) => {
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
                self.heartbeat_deadline = Instant::now() + Duration::from_secs(30);
            }
            AdapterEvent::Telemetry(sample) => {
                self.heartbeat_deadline = Instant::now() + Duration::from_secs(30);
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
            AdapterEvent::Heartbeat => {
                self.heartbeat_deadline = Instant::now() + Duration::from_secs(30);
            }
            AdapterEvent::None => {}
        }

        Ok(notification)
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
        ) {
            let _ = std::fs::remove_file(&abs);
            return Err(err);
        }
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
        self.heartbeat_deadline = Instant::now() + Duration::from_secs(365 * 24 * 3600);
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if !paused {
            self.heartbeat_deadline = Instant::now() + Duration::from_secs(5);
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
