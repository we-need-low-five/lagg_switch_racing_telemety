use crate::ac_maps::{AcGraphics, AcPhysics, AcStatics, GRAPHICS_NAME, PHYSICS_NAME, STATIC_NAME};
use chrono::Utc;
use sim_capture_common::SharedMemoryView;
use sim_core::{
    kmh_to_mps, normalize_brake, normalize_steering, normalize_throttle, AdapterEvent,
    GameAdapter, GameId, LapSummary, SectorTimes, SessionInfo, TelemetrySample,
};

pub struct AcAdapter {
    physics: Option<SharedMemoryView<AcPhysics>>,
    graphics: Option<SharedMemoryView<AcGraphics>>,
    statics: Option<SharedMemoryView<AcStatics>>,
    last_completed_laps: i32,
    session_announced: bool,
    sector_times: SectorTimes,
    last_sector_index: i32,
    current_lap_in_pit: bool,
}

impl AcAdapter {
    pub fn new() -> Self {
        Self {
            physics: None,
            graphics: None,
            statics: None,
            last_completed_laps: -1,
            session_announced: false,
            sector_times: SectorTimes {
                s1_ms: None,
                s2_ms: None,
                s3_ms: None,
            },
            last_sector_index: -1,
            current_lap_in_pit: false,
        }
    }

    fn connect(&mut self) -> bool {
        if self.physics.is_some() {
            return true;
        }
        self.physics = SharedMemoryView::<AcPhysics>::open(PHYSICS_NAME, 0).ok();
        self.graphics = SharedMemoryView::<AcGraphics>::open(GRAPHICS_NAME, 0).ok();
        self.statics = SharedMemoryView::<AcStatics>::open(STATIC_NAME, 0).ok();
        self.physics.is_some() && self.graphics.is_some() && self.statics.is_some()
    }
}

impl Default for AcAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GameAdapter for AcAdapter {
    fn game_id(&self) -> GameId {
        GameId::Ac
    }

    fn is_active(&self) -> bool {
        self.physics.is_some()
    }

    fn poll(&mut self) -> AdapterEvent {
        if !self.connect() {
            return AdapterEvent::Disconnected;
        }

        let physics = self.physics.as_ref().unwrap().read();
        let graphics = self.graphics.as_ref().unwrap().read();
        let statics = self.statics.as_ref().unwrap().read();

        if !self.session_announced {
            self.session_announced = true;
            return AdapterEvent::SessionInfo(SessionInfo {
                game: GameId::Ac,
                track_id: slugify_track_id(&statics.track_name()),
                track: statics.track_name(),
                car: statics.car_name(),
                game_version: statics.game_version(),
                player_name: statics.player_display(),
            });
        }

        if self.last_completed_laps >= 0 && graphics.completed_laps > self.last_completed_laps {
            let lap_number = self.last_completed_laps + 1;
            let lap_time_ms = graphics.i_last_time.max(0) as u32;

            let sectors = sim_core::acc_cumulative_splits_to_sectors(
                self.sector_times.s1_ms,
                self.sector_times.s2_ms,
                lap_time_ms,
            );

            // AC has no is_valid_lap; invalidate if the car visited pit this lap.
            let valid = lap_time_ms > 0 && !self.current_lap_in_pit;
            let summary = LapSummary {
                lap_number: lap_number as u32,
                lap_time_ms,
                valid,
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
            self.last_sector_index = -1;
            self.current_lap_in_pit = false;
            self.last_completed_laps = graphics.completed_laps;
            return AdapterEvent::LapCompleted(summary);
        }

        if self.last_completed_laps < 0 {
            self.last_completed_laps = graphics.completed_laps;
            self.last_sector_index = graphics.current_sector_index;
            self.current_lap_in_pit = graphics.is_in_pit != 0;
            if graphics.completed_laps == 0 {
                return AdapterEvent::LapStarted { lap_number: 1 };
            }
        }

        if graphics.is_in_pit != 0 {
            self.current_lap_in_pit = true;
        }

        if graphics.current_sector_index != self.last_sector_index
            && graphics.last_sector_time > 0
        {
            // AC current_sector_index is 0-based (0=S1, 1=S2, 2=S3), same as ACC.
            match self.last_sector_index {
                0 => self.sector_times.s1_ms = Some(graphics.last_sector_time as u32),
                1 => self.sector_times.s2_ms = Some(graphics.last_sector_time as u32),
                _ => {}
            }
            self.last_sector_index = graphics.current_sector_index;
        }

        self.last_completed_laps = graphics.completed_laps;

        AdapterEvent::Telemetry(TelemetrySample {
            timestamp: Utc::now(),
            lap_time_s: graphics.i_current_time.max(0) as f32 / 1000.0,
            distance_m: graphics.distance_traveled,
            speed_mps: kmh_to_mps(physics.speed_kmh),
            throttle: normalize_throttle(physics.gas),
            brake: normalize_brake(physics.brake),
            // AC physics.steer_angle is wheel degrees; same 450° lock as ACC.
            steering: normalize_steering(physics.steer_angle / 450.0),
            gear: physics.gear,
            rpm: physics.rpm as f32,
            // Original AC graphics has no carCoordinates (ACC-only). Leave
            // world position at origin and resample from distance_traveled.
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            fuel: Some(physics.fuel),
            tyre_temp_fl: Some(physics.tyre_core_temperature[0]),
            tyre_temp_fr: Some(physics.tyre_core_temperature[1]),
            tyre_temp_rl: Some(physics.tyre_core_temperature[2]),
            tyre_temp_rr: Some(physics.tyre_core_temperature[3]),
            tyre_press_fl: Some(physics.wheels_pressure[0]),
            tyre_press_fr: Some(physics.wheels_pressure[1]),
            tyre_press_rl: Some(physics.wheels_pressure[2]),
            tyre_press_rr: Some(physics.wheels_pressure[3]),
            raw: serde_json::json!({
                "wheel_slip": physics.wheel_slip,
                "velocity": physics.velocity,
                "is_in_pit": graphics.is_in_pit,
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
