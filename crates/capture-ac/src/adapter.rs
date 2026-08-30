use crate::ac_maps::{AcGraphics, AcPhysics, AcStatics, GRAPHICS_NAME, PHYSICS_NAME, STATIC_NAME};
use chrono::Utc;
use sim_capture_common::SharedMemoryView;
use sim_core::{
    kmh_to_mps, normalize_brake, normalize_steering, normalize_throttle, AdapterEvent,
    GameAdapter, GameId, LapSummary, SectorTimes, SessionInfo, SessionKind, TelemetrySample,
};

/// Sentinel for "no `graphics.session` observed yet".
const AC_SESSION_UNSET: i32 = i32::MIN;

/// AC `graphics.session` (`AC_SESSION_TYPE`): -1 unknown, 0 practice, 1 qualify,
/// 2 race, 3 hotlap, 4 time attack, 5 drift, 6 drag.
fn ac_session_kind(session: i32) -> SessionKind {
    match session {
        0 => SessionKind::Practice,
        1 => SessionKind::Qualifying,
        2 => SessionKind::Race,
        3 => SessionKind::Hotlap,
        4 => SessionKind::TimeAttack,
        5 | 6 => SessionKind::Other,
        _ => SessionKind::Unknown,
    }
}

pub struct AcAdapter {
    physics: Option<SharedMemoryView<AcPhysics>>,
    graphics: Option<SharedMemoryView<AcGraphics>>,
    statics: Option<SharedMemoryView<AcStatics>>,
    last_completed_laps: i32,
    session_announced: bool,
    last_track_id: String,
    last_car: String,
    /// `graphics.session` at the last announce. A change is a session boundary
    /// even on the same track/car (each phase restarts the lap counter).
    last_session: i32,
    last_packet_id: i32,
    stale_packet_polls: u32,
    sector_times: SectorTimes,
    last_sector_index: i32,
    current_lap_in_pit: bool,
}

/// AC `graphics.status`: 0 OFF, 1 REPLAY, 2 LIVE, 3 PAUSE. We only treat LIVE
/// as "on track" for session/track-change decisions.
const AC_STATUS_LIVE: i32 = 2;

/// Polls a frozen `packet_id` may persist before we assume telemetry is still
/// live (some AC builds stop advancing packetId even while driving). ~90 ms at
/// the 3 ms recorder poll.
const STALE_PACKET_LIMIT: u32 = 30;

fn ac_car_is_moving(physics: &AcPhysics) -> bool {
    physics.speed_kmh > 0.5 || physics.gas > 0.01 || physics.brake > 0.01
}

/// A `packet_id` that has stopped advancing normally means "paused" — but if it
/// stays frozen past the limit while the car is moving, the build just isn't
/// advancing packetId and we should keep emitting telemetry.
fn frozen_packet_still_live(stale_polls: u32, moving: bool) -> bool {
    stale_polls >= STALE_PACKET_LIMIT && moving
}

impl AcAdapter {
    pub fn new() -> Self {
        Self {
            physics: None,
            graphics: None,
            statics: None,
            last_completed_laps: -1,
            session_announced: false,
            last_track_id: String::new(),
            last_car: String::new(),
            last_session: AC_SESSION_UNSET,
            last_packet_id: -1,
            stale_packet_polls: 0,
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
            // Wait until the sim is on track — announcing from the menu/replay
            // creates empty sessions and, on a later track load, a bogus split.
            if graphics.status != AC_STATUS_LIVE {
                return AdapterEvent::Heartbeat;
            }
            self.session_announced = true;
            self.last_track_id = slugify_track_id(&statics.track_name());
            self.last_car = statics.car_name();
            self.last_session = graphics.session;
            self.last_packet_id = physics.packet_id;
            return AdapterEvent::SessionInfo(SessionInfo {
                game: GameId::Ac,
                track_id: self.last_track_id.clone(),
                track: statics.track_name(),
                car: self.last_car.clone(),
                game_version: statics.game_version(),
                player_name: statics.player_display(),
                session_kind: ac_session_kind(graphics.session),
            });
        }

        let track_id = slugify_track_id(&statics.track_name());
        let car = statics.car_name();
        let track_changed = !track_id.is_empty()
            && !self.last_track_id.is_empty()
            && !self.last_track_id.eq_ignore_ascii_case(&track_id);
        let car_changed = !car.trim().is_empty()
            && !self.last_car.trim().is_empty()
            && !self.last_car.eq_ignore_ascii_case(car.trim());
        // Each weekend phase (practice → qualify → race) restarts AC's lap
        // counter, so a `graphics.session` change is its own session boundary.
        let session_changed =
            self.last_session != AC_SESSION_UNSET && graphics.session != self.last_session;
        if graphics.status == AC_STATUS_LIVE && (track_changed || car_changed || session_changed) {
            self.last_track_id = track_id.clone();
            self.last_car = car.clone();
            self.last_session = graphics.session;
            self.last_completed_laps = -1;
            self.last_packet_id = physics.packet_id;
            self.stale_packet_polls = 0;
            self.last_sector_index = -1;
            self.current_lap_in_pit = false;
            return AdapterEvent::SessionInfo(SessionInfo {
                game: GameId::Ac,
                track_id,
                track: statics.track_name(),
                car,
                game_version: statics.game_version(),
                player_name: statics.player_display(),
                session_kind: ac_session_kind(graphics.session),
            });
        }

        if physics.packet_id == self.last_packet_id {
            self.stale_packet_polls = self.stale_packet_polls.saturating_add(1);
            if !frozen_packet_still_live(self.stale_packet_polls, ac_car_is_moving(&physics)) {
                return AdapterEvent::Heartbeat;
            }
            // packetId is frozen but the car is clearly live — keep recording.
        } else {
            self.stale_packet_polls = 0;
        }
        self.last_packet_id = physics.packet_id;

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
            // AC physics.steer_angle is steering-wheel degrees; persist as
            // fraction of 450° lock for L/R % charts.
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
            g_force_x: Some(physics.acc_g[0]),
            g_force_y: Some(physics.acc_g[1]),
            g_force_z: Some(physics.acc_g[2]),
            slip_angle_fl: None,
            slip_angle_fr: None,
            slip_angle_rl: None,
            slip_angle_rr: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stationary_physics() -> AcPhysics {
        // AcPhysics is `#[repr(C)]` POD; an all-zero frame is a valid parked car.
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn stationary_frame_is_not_moving() {
        assert!(!ac_car_is_moving(&stationary_physics()));
    }

    #[test]
    fn throttle_brake_or_speed_counts_as_moving() {
        let mut p = stationary_physics();
        p.speed_kmh = 12.0;
        assert!(ac_car_is_moving(&p));

        let mut p = stationary_physics();
        p.gas = 0.2;
        assert!(ac_car_is_moving(&p));

        let mut p = stationary_physics();
        p.brake = 0.5;
        assert!(ac_car_is_moving(&p));
    }

    #[test]
    fn frozen_packet_is_paused_until_limit_then_live_only_if_moving() {
        assert!(!frozen_packet_still_live(1, true), "brief freeze = paused");
        assert!(
            !frozen_packet_still_live(STALE_PACKET_LIMIT, false),
            "long freeze while parked stays paused"
        );
        assert!(
            frozen_packet_still_live(STALE_PACKET_LIMIT, true),
            "long freeze while moving = keep recording"
        );
    }

    #[test]
    fn slugify_track_id_normalizes() {
        assert_eq!(slugify_track_id("  Spa-Francorchamps "), "spafrancorchamps");
        assert_eq!(slugify_track_id("ks_monza"), "ks_monza");
    }
}
