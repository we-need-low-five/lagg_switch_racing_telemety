use crate::lmu_maps::{LmuTelemetry, LMU_DATA_NAME};
use chrono::Utc;
use sim_capture_common::SharedMemoryView;
use sim_core::{
    normalize_brake, normalize_steering, normalize_throttle, AdapterEvent, GameAdapter, GameId,
    LapSummary, SectorTimes, SessionInfo, TelemetrySample,
};

pub struct LmuAdapter {
    telemetry: Option<SharedMemoryView<LmuTelemetry>>,
    last_lap: i32,
    session_announced: bool,
    last_track_id: String,
    last_car: String,
    sector_times: SectorTimes,
}

impl LmuAdapter {
    pub fn new() -> Self {
        Self {
            telemetry: None,
            last_lap: -1,
            session_announced: false,
            last_track_id: String::new(),
            last_car: String::new(),
            sector_times: SectorTimes {
                s1_ms: None,
                s2_ms: None,
                s3_ms: None,
            },
        }
    }

    fn connect(&mut self) -> bool {
        if self.telemetry.is_some() {
            return true;
        }
        self.telemetry = SharedMemoryView::<LmuTelemetry>::open(LMU_DATA_NAME, 0).ok();
        self.telemetry.is_some()
    }
}

impl Default for LmuAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GameAdapter for LmuAdapter {
    fn game_id(&self) -> GameId {
        GameId::Lmu
    }

    fn is_active(&self) -> bool {
        self.telemetry.is_some()
    }

    fn poll(&mut self) -> AdapterEvent {
        if !self.connect() {
            return AdapterEvent::Disconnected;
        }

        let data = self.telemetry.as_ref().unwrap().read();
        if data.active == 0 {
            if self.session_announced {
                return AdapterEvent::Heartbeat;
            }
            return AdapterEvent::Disconnected;
        }

        let track_id = slugify_track_id(&data.track());
        let car = data.vehicle();
        if !self.session_announced {
            self.session_announced = true;
            self.last_track_id = track_id.clone();
            self.last_car = car.clone();
            return AdapterEvent::SessionInfo(SessionInfo {
                game: GameId::Lmu,
                track_id,
                track: data.track(),
                car,
                game_version: format!("LMU v{}", data.version),
                player_name: data.player(),
            });
        }

        let track_changed = !track_id.is_empty()
            && !self.last_track_id.is_empty()
            && !self.last_track_id.eq_ignore_ascii_case(&track_id);
        let car_changed = !car.trim().is_empty()
            && !self.last_car.trim().is_empty()
            && !self.last_car.eq_ignore_ascii_case(car.trim());
        if track_changed || car_changed {
            self.last_track_id = track_id.clone();
            self.last_car = car.clone();
            self.last_lap = -1;
            return AdapterEvent::SessionInfo(SessionInfo {
                game: GameId::Lmu,
                track_id,
                track: data.track(),
                car,
                game_version: format!("LMU v{}", data.version),
                player_name: data.player(),
            });
        }

        if data.sector_time > 0.0 {
            let ms = (data.sector_time * 1000.0) as u32;
            match data.sector {
                1 => self.sector_times.s1_ms = Some(ms),
                2 => self.sector_times.s2_ms = Some(ms),
                3 => self.sector_times.s3_ms = Some(ms),
                _ => {}
            }
        }

        if self.last_lap >= 0 && data.lap_number > self.last_lap {
            let lap_time_ms = (data.last_lap_time * 1000.0) as u32;
            let sectors = sim_core::normalize_sector_times(&self.sector_times, lap_time_ms);
            let summary = LapSummary {
                lap_number: self.last_lap as u32,
                lap_time_ms,
                valid: lap_time_ms > 0 && data.in_pits == 0,
                sectors,
                tyre_compound: None,
                tc_level: None,
                abs_level: None,
                fuel_used_l: None,
            };
            self.sector_times = SectorTimes {
                s1_ms: None,
                s2_ms: None,
                s3_ms: None,
            };
            self.last_lap = data.lap_number;
            return AdapterEvent::LapCompleted(summary);
        }

        if self.last_lap < 0 {
            self.last_lap = data.lap_number;
            return AdapterEvent::LapStarted {
                lap_number: data.lap_number.max(1) as u32,
            };
        }

        self.last_lap = data.lap_number;

        AdapterEvent::Telemetry(TelemetrySample {
            timestamp: Utc::now(),
            lap_time_s: data.current_time as f32,
            distance_m: 0.0,
            speed_mps: data.speed as f32,
            throttle: normalize_throttle(data.throttle as f32),
            brake: normalize_brake(data.brake as f32),
            steering: normalize_steering(data.steering as f32),
            gear: data.gear,
            rpm: data.engine_rpm as f32,
            pos_x: data.pos_x as f32,
            pos_y: data.pos_y as f32,
            pos_z: data.pos_z as f32,
            fuel: None,
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
            raw: serde_json::json!({
                "clutch": data.clutch,
                "best_lap_time": data.best_lap_time,
                "in_pits": data.in_pits,
            }),
        })
    }
}

fn slugify_track_id(track: &str) -> String {
    track
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "_")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}
