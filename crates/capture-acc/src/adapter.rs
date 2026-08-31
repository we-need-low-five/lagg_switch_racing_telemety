use crate::acc_statics::{AccStaticsSnapshot, STATIC_NAME, STATIC_SIZE};

use acc_shared_memory_rs::core::SharedMemoryReader;

use acc_shared_memory_rs::enums::AccSessionType;

use acc_shared_memory_rs::maps::GraphicsMap;

use acc_shared_memory_rs::parsers::{parse_graphics_map, parse_physics_map};

use chrono::Utc;

use sim_capture_common::SharedMemoryMapping;

use sim_core::{

    kmh_to_mps, normalize_brake, normalize_throttle, AdapterEvent,

    GameAdapter, GameId, LapSummary, PitCycleDetector, SectorTimes, SessionInfo, SessionKind,
    TelemetrySample,

};

/// Map ACC's session type onto the sim-agnostic [`SessionKind`].
fn acc_session_kind(session_type: AccSessionType) -> SessionKind {
    match session_type {
        AccSessionType::Practice => SessionKind::Practice,
        AccSessionType::Qualifying | AccSessionType::HotlapSuperpole => SessionKind::Qualifying,
        AccSessionType::Race => SessionKind::Race,
        AccSessionType::Hotlap => SessionKind::Hotlap,
        AccSessionType::TimeAttack => SessionKind::TimeAttack,
        AccSessionType::Drift | AccSessionType::Drag | AccSessionType::Hotstint => SessionKind::Other,
        AccSessionType::Unknown => SessionKind::Unknown,
    }
}

/// Sentinel for "no session type observed yet" in the adapter's last-seen state.
const SESSION_UNSET: i32 = i32::MIN;



const PHYSICS_NAME: &str = "Local\\acpmf_physics";

const PHYSICS_SIZE: usize = 800;

const GRAPHICS_NAME: &str = "Local\\acpmf_graphics";

const GRAPHICS_SIZE: usize = 1588;



pub fn acc_telemetry_available() -> bool {

    SharedMemoryMapping::is_open(PHYSICS_NAME, PHYSICS_SIZE)

        && SharedMemoryMapping::is_open(STATIC_NAME, STATIC_SIZE)

        && SharedMemoryMapping::is_open(GRAPHICS_NAME, GRAPHICS_SIZE)

}



pub struct AccAdapter {

    physics_reader: Option<SharedMemoryReader>,

    graphics_reader: Option<SharedMemoryReader>,

    statics_mapping: Option<SharedMemoryMapping>,

    last_physics_packet: i32,

    last_completed_laps: i32,

    sector_times: SectorTimes,

    last_sector_index: i32,

    session_announced: bool,

    last_track_id: String,

    last_car: String,

    /// ACC `graphics.session_type` / `session_index` at the last announce. ACC
    /// keeps the same track and car across Practice → Qualifying → Race but
    /// restarts `completed_lap` each phase, so a change here is a real session
    /// boundary. `SESSION_UNSET` until the first announce.
    last_session_type: i32,

    last_session_index: i32,

    current_lap_valid: bool,

    current_lap_in_pit: bool,

    lap_start_compound: Option<String>,

    lap_start_tc: Option<i32>,

    lap_start_abs: Option<i32>,

    lap_start_meta_captured: bool,

    /// Adapter-owned lap counter. ACC's `completed_lap` stalls or restarts
    /// across out-laps and return-to-garage, so lap numbers are minted here on
    /// each emitted lap instead. Reset only on a real session / phase change.
    lap_counter: u32,

    /// `normalized_car_position` on the previous poll (`-1.0` until first read).
    /// A high→low wrap is the start/finish line and drives the lap boundary
    /// even when `completed_lap` never moved (unscored out-laps).
    last_norm_pos: f32,

    /// Largest `current_time` (live lap timer, ms) seen since the last
    /// boundary. Fallback lap time when ACC's `last_time` is 0 / a sentinel.
    max_current_time_ms: u32,

    /// False while a just-taken boundary is still settling; blocks the
    /// `completed_lap` delta and the S/F wrap from firing twice for one lap.
    boundary_settled: bool,

    /// Return-to-garage / pit-stop → new stint, off the `is_in_pit_lane` edge.
    pit_cycle: PitCycleDetector,

}



impl AccAdapter {

    pub fn new() -> Self {

        Self {

            physics_reader: None,

            graphics_reader: None,

            statics_mapping: None,

            last_physics_packet: -1,

            last_completed_laps: -1,

            sector_times: SectorTimes {

                s1_ms: None,

                s2_ms: None,

                s3_ms: None,

            },

            last_sector_index: -1,

            session_announced: false,

            last_track_id: String::new(),

            last_car: String::new(),

            last_session_type: SESSION_UNSET,

            last_session_index: SESSION_UNSET,

            current_lap_valid: true,

            current_lap_in_pit: false,

            lap_start_compound: None,

            lap_start_tc: None,

            lap_start_abs: None,

            lap_start_meta_captured: false,

            lap_counter: 0,

            last_norm_pos: -1.0,

            max_current_time_ms: 0,

            boundary_settled: true,

            pit_cycle: PitCycleDetector::new(),

        }

    }



    fn connect(&mut self) -> bool {

        if self.physics_reader.is_some() {

            return true;

        }

        let physics_reader = match SharedMemoryReader::new(PHYSICS_NAME, PHYSICS_SIZE) {

            Ok(reader) => reader,

            Err(_) => return false,

        };

        let graphics_reader = match SharedMemoryReader::new(GRAPHICS_NAME, GRAPHICS_SIZE) {

            Ok(reader) => reader,

            Err(_) => return false,

        };

        let statics_mapping = match SharedMemoryMapping::open(STATIC_NAME, STATIC_SIZE) {

            Ok(mapping) => mapping,

            Err(_) => return false,

        };

        self.physics_reader = Some(physics_reader);

        self.graphics_reader = Some(graphics_reader);

        self.statics_mapping = Some(statics_mapping);

        true

    }



    fn disconnect(&mut self) {

        self.physics_reader = None;

        self.graphics_reader = None;

        self.statics_mapping = None;

        self.session_announced = false;

        self.last_track_id.clear();

        self.last_car.clear();

        self.last_session_type = SESSION_UNSET;

        self.last_session_index = SESSION_UNSET;

        self.last_completed_laps = -1;

        self.last_physics_packet = -1;

        self.last_sector_index = -1;

        self.current_lap_valid = true;

        self.current_lap_in_pit = false;

        self.lap_start_compound = None;

        self.lap_start_tc = None;

        self.lap_start_abs = None;

        self.lap_start_meta_captured = false;

        self.lap_counter = 0;

        self.last_norm_pos = -1.0;

        self.max_current_time_ms = 0;

        self.boundary_settled = true;

        self.pit_cycle.reset();

        self.sector_times = SectorTimes {

            s1_ms: None,

            s2_ms: None,

            s3_ms: None,

        };

    }



    fn read_graphics(&self) -> Option<GraphicsMap> {

        let reader = self.graphics_reader.as_ref()?;

        parse_graphics_map(reader).ok()

    }



    fn read_statics(&self) -> Option<AccStaticsSnapshot> {

        let mapping = self.statics_mapping.as_ref()?;

        Some(AccStaticsSnapshot::read(mapping))

    }



    fn capture_lap_start_meta(&mut self, graphics: &GraphicsMap) {
        if self.lap_start_meta_captured {
            return;
        }
        let compound = graphics.tyre_compound.trim();
        self.lap_start_compound = if compound.is_empty() {
            None
        } else {
            Some(compound.to_string())
        };
        self.lap_start_tc = Some(graphics.tc_level);
        self.lap_start_abs = Some(graphics.abs_level);
        self.lap_start_meta_captured = true;
    }



    fn update_sector_times(&mut self, graphics: &GraphicsMap) {
        if graphics.current_sector_index != self.last_sector_index && graphics.last_sector_time > 0
        {
            // ACC current_sector_index is 0-based (0=S1, 1=S2, 2=S3).
            // last_sector_time at S1/S2 lines is cumulative elapsed; S3 is derived at lap end.
            match self.last_sector_index {
                0 => self.sector_times.s1_ms = Some(graphics.last_sector_time as u32),
                1 => self.sector_times.s2_ms = Some(graphics.last_sector_time as u32),
                _ => {}
            }
            self.last_sector_index = graphics.current_sector_index;
        }
    }



    fn track_lap_state(&mut self, graphics: &GraphicsMap, speed_kmh: f32) {

        // ACC keeps `is_valid_lap` at 0 whenever no flying lap is being timed —
        // stationary, in the pit lane, or in the instant after crossing the
        // line before the next lap's timing starts. A stopped car can't be
        // invalidating the lap it already finished, so only honour the flag
        // while the car is actually running a lap.
        if !graphics.is_valid_lap && speed_kmh > 5.0 && !graphics.is_in_pit_lane {

            self.current_lap_valid = false;

        }

        if graphics.is_in_pit_lane || graphics.is_in_pit {

            self.current_lap_in_pit = true;

        }

    }



    /// Emit the lap that just ended at a start/finish boundary. `poll` calls
    /// this once per crossing, whether the boundary was seen as an ACC
    /// `completed_lap` increment or as a `normalized_car_position` wrap.
    ///
    /// `lap_valid_snapshot` is `current_lap_valid` as of the previous tick — on
    /// the boundary tick ACC's live `is_valid_lap` already describes the next
    /// lap, so the finished lap is judged on the pre-tick value.
    ///
    /// Per-lap state is cleared even when no lap is emitted (no usable time,
    /// e.g. the first wrap straight out of the garage).
    fn finish_lap(
        &mut self,
        graphics: &GraphicsMap,
        lap_valid_snapshot: bool,
    ) -> Option<AdapterEvent> {

        // ACC reports `last_time` as 0 or a huge sentinel until a lap has been
        // scored this run (out-laps, first lap after a return-to-garage). Fall
        // back to the live lap timer's peak for those.
        let acc_last = graphics.last_time;
        let lap_time_ms = if (1_000..=1_800_000).contains(&acc_last) {
            acc_last as u32
        } else {
            self.max_current_time_ms
        };

        let in_pit = self.current_lap_in_pit;
        let sectors = sim_core::acc_cumulative_splits_to_sectors(
            self.sector_times.s1_ms,
            self.sector_times.s2_ms,
            lap_time_ms,
        );
        let tyre_compound = self.lap_start_compound.take();
        let tc_level = self.lap_start_tc.take();
        let abs_level = self.lap_start_abs.take();

        // Reset per-lap state regardless of whether a lap is emitted.
        self.lap_start_meta_captured = false;
        self.sector_times = SectorTimes {
            s1_ms: None,
            s2_ms: None,
            s3_ms: None,
        };
        self.last_sector_index = graphics.current_sector_index;
        self.current_lap_valid = true;
        self.current_lap_in_pit = false;
        self.max_current_time_ms = 0;

        if lap_time_ms < 1_000 {
            return None;
        }

        self.lap_counter += 1;
        // Out / in-laps are recorded (tagged invalid) but don't arm the
        // return-to-garage stint split.
        self.pit_cycle.lap_completed(in_pit);

        Some(AdapterEvent::LapCompleted(LapSummary {
            lap_number: self.lap_counter,
            lap_time_ms,
            valid: lap_valid_snapshot && !in_pit,
            sectors,
            tyre_compound,
            tc_level,
            abs_level,
            fuel_used_l: None,
        }))
    }



    fn reset_lap_progress(&mut self) {

        self.last_completed_laps = -1;

        self.last_physics_packet = -1;

        self.last_sector_index = -1;

        self.current_lap_valid = true;

        self.current_lap_in_pit = false;

        self.lap_start_compound = None;

        self.lap_start_tc = None;

        self.lap_start_abs = None;

        self.lap_start_meta_captured = false;

        // Per-lap transients only. `lap_counter` and `pit_cycle` deliberately
        // survive the AC_PAUSE blip that a return-to-garage triggers, so the
        // pit-out stint split still fires.
        self.last_norm_pos = -1.0;

        self.max_current_time_ms = 0;

        self.boundary_settled = true;

        self.sector_times = SectorTimes {

            s1_ms: None,

            s2_ms: None,

            s3_ms: None,

        };

    }



    /// Clear the recording-scoped lap/stint counters. Called only on a real
    /// session boundary (first announce, or a track / car / weekend-phase
    /// change) — never on the transient AC_PAUSE seen during return-to-garage.
    fn reset_stint_tracking(&mut self) {
        self.lap_counter = 0;
        self.pit_cycle.reset();
    }



    fn session_info(graphics: &GraphicsMap, statics: &AccStaticsSnapshot) -> Option<SessionInfo> {

        let track = statics.track_name()?;

        let car = statics.car_model.trim();

        if car.is_empty() {

            return None;

        }



        Some(SessionInfo {

            game: GameId::Acc,

            track_id: statics.track.trim().to_ascii_lowercase(),

            track,

            car: car.to_string(),

            game_version: statics.ac_version.clone(),

            player_name: statics.player_display(),

            session_kind: acc_session_kind(graphics.session_type),

        })

    }



    fn session_ready(graphics: &GraphicsMap, statics: &AccStaticsSnapshot) -> bool {

        graphics.status.is_active() && Self::session_info(graphics, statics).is_some()

    }

}



impl Default for AccAdapter {

    fn default() -> Self {

        Self::new()

    }

}



impl GameAdapter for AccAdapter {

    fn game_id(&self) -> GameId {

        GameId::Acc

    }



    fn is_active(&self) -> bool {

        self.physics_reader.is_some()

    }



    fn detects_pit_stints(&self) -> bool {
        true
    }



    fn poll(&mut self) -> AdapterEvent {

        if !self.connect() {

            return AdapterEvent::Disconnected;

        }



        let graphics = match self.read_graphics() {

            Some(graphics) => graphics,

            None => {

                self.disconnect();

                return AdapterEvent::Disconnected;

            }

        };



        let statics = match self.read_statics() {

            Some(statics) => statics,

            None => {

                self.disconnect();

                return AdapterEvent::Disconnected;

            }

        };



        if !self.session_announced {

            if !Self::session_ready(&graphics, &statics) {

                return AdapterEvent::Heartbeat;

            }

            self.session_announced = true;

            self.reset_stint_tracking();

            self.last_completed_laps = graphics.completed_lap;

            self.last_sector_index = graphics.current_sector_index;

            if let Ok(physics) = parse_physics_map(self.physics_reader.as_ref().unwrap()) {

                self.last_physics_packet = physics.packet_id;

            }

            let info = Self::session_info(&graphics, &statics)
                .expect("session_ready guarantees session info");

            self.last_track_id = info.track_id.clone();

            self.last_car = info.car.clone();

            self.last_session_type = graphics.session_type as i32;

            self.last_session_index = graphics.session_index;

            return AdapterEvent::SessionInfo(info);

        }



        if !Self::session_ready(&graphics, &statics) {

            self.reset_lap_progress();

            return AdapterEvent::Heartbeat;

        }



        // Only treat a track_id / car / session-type change as a real session
        // boundary while the sim is actually on track. Menu navigation flips
        // statics fields without a live session and must not spawn empty
        // sessions downstream.

        if let Some(info) = Self::session_info(&graphics, &statics) {

            let track_changed = !self.last_track_id.is_empty()

                && !info.track_id.trim().is_empty()

                && !self.last_track_id.eq_ignore_ascii_case(&info.track_id);

            let car_changed = !self.last_car.is_empty()

                && !info.car.trim().is_empty()

                && !self.last_car.eq_ignore_ascii_case(info.car.trim());

            // ACC restarts `completed_lap` at 0 for each phase of a weekend, so
            // Practice → Qualifying → Race on the same track/car needs its own
            // session or the phases' lap numbers collide downstream.
            let session_changed = self.last_session_type != SESSION_UNSET
                && (graphics.session_type as i32 != self.last_session_type
                    || graphics.session_index != self.last_session_index);

            if track_changed || car_changed || session_changed {

                self.reset_lap_progress();

                self.reset_stint_tracking();

                self.last_track_id = info.track_id.clone();

                self.last_car = info.car.clone();

                self.last_session_type = graphics.session_type as i32;

                self.last_session_index = graphics.session_index;

                return AdapterEvent::SessionInfo(info);

            }

        }



        if self.last_completed_laps < 0 {

            self.last_completed_laps = graphics.completed_lap;

            self.last_sector_index = graphics.current_sector_index;

            if let Ok(physics) = parse_physics_map(self.physics_reader.as_ref().unwrap()) {

                self.last_physics_packet = physics.packet_id;

            }

        }



        let physics = match parse_physics_map(self.physics_reader.as_ref().unwrap()) {

            Ok(physics) => physics,

            Err(_) => {

                self.disconnect();

                return AdapterEvent::Disconnected;

            }

        };



        // Validity as of the previous tick, before this tick's reading is
        // folded in. The completion verdict below uses this so a post-line
        // `is_valid_lap == 0` (which belongs to the next lap) can't retroactively
        // invalidate the lap that just finished.
        let lap_valid_before_tick = self.current_lap_valid;

        self.update_sector_times(&graphics);

        self.track_lap_state(&graphics, physics.speed_kmh);

        self.capture_lap_start_meta(&graphics);

        // Running peak of the live lap timer — fallback lap time for a wrap
        // that ACC never scored (out-laps).
        self.max_current_time_ms = self
            .max_current_time_ms
            .max(graphics.current_time.max(0) as u32);



        // Lap boundary: the car wrapped past the start/finish line, or ACC
        // scored a lap we didn't catch as a wrap (a poll gap). Take it once per
        // crossing — `boundary_settled` clears until the car is unambiguously
        // into the next lap so the two signals can't double-fire.
        let norm = graphics.normalized_car_position;
        // `last_norm_pos` starts at -1.0, so `> 0.90` also rules out the sentinel.
        let crossed_sf = self.last_norm_pos > 0.90 && norm < 0.10;
        self.last_norm_pos = norm;

        let acc_scored = self.last_completed_laps >= 0
            && graphics.completed_lap > self.last_completed_laps;

        if (crossed_sf || acc_scored) && self.boundary_settled {
            self.boundary_settled = false;
            self.last_completed_laps = graphics.completed_lap;
            if let Some(event) = self.finish_lap(&graphics, lap_valid_before_tick) {
                return event;
            }
        } else if acc_scored {
            // Boundary already taken for this crossing; keep the tracker in step.
            self.last_completed_laps = graphics.completed_lap;
        }
        if (0.20..0.80).contains(&norm) {
            self.boundary_settled = true;
        }



        // Return-to-garage / pit stop: once the car is back out of the pit lane
        // and has at least one flying lap behind it this stint, roll a new one.
        if self
            .pit_cycle
            .left_pits(graphics.is_in_pit || graphics.is_in_pit_lane)
        {
            // The out-lap now starting belongs to the new stint.
            self.current_lap_valid = true;
            return AdapterEvent::StintBoundary;
        }



        if physics.packet_id == self.last_physics_packet {

            return AdapterEvent::Heartbeat;

        }

        self.last_physics_packet = physics.packet_id;



        let pos = player_position(&graphics);

        AdapterEvent::Telemetry(TelemetrySample {

            timestamp: Utc::now(),

            lap_time_s: graphics.current_time.max(0) as f32 / 1000.0,

            distance_m: graphics.distance_traveled,

            speed_mps: kmh_to_mps(physics.speed_kmh),

            throttle: normalize_throttle(physics.gas),

            brake: normalize_brake(physics.brake),

            // ACC steer_angle is a ~−1…1 input, not wheel degrees. Scale so
            // full lock is ±100° (Motec-style). Do not use a 450° lock.
            steering: physics.steer_angle * 100.0,

            gear: physics.gear,

            rpm: physics.rpm as f32,

            pos_x: pos[0],

            pos_y: pos[1],

            pos_z: pos[2],

            fuel: Some(physics.fuel),

            tyre_temp_fl: Some(physics.tyre_core_temp.front_left),

            tyre_temp_fr: Some(physics.tyre_core_temp.front_right),

            tyre_temp_rl: Some(physics.tyre_core_temp.rear_left),

            tyre_temp_rr: Some(physics.tyre_core_temp.rear_right),

            tyre_press_fl: Some(physics.wheel_pressure.front_left),

            tyre_press_fr: Some(physics.wheel_pressure.front_right),

            tyre_press_rl: Some(physics.wheel_pressure.rear_left),

            tyre_press_rr: Some(physics.wheel_pressure.rear_right),

            g_force_x: Some(physics.g_force.x),

            g_force_y: Some(physics.g_force.y),

            g_force_z: Some(physics.g_force.z),

            slip_angle_fl: Some(physics.slip_angle.front_left),

            slip_angle_fr: Some(physics.slip_angle.front_right),

            slip_angle_rl: Some(physics.slip_angle.rear_left),

            slip_angle_rr: Some(physics.slip_angle.rear_right),

            raw: serde_json::json!({

                "physics": {

                    "packet_id": physics.packet_id,

                    "fuel": physics.fuel,

                    "heading": physics.heading,

                    "pitch": physics.pitch,

                    "roll": physics.roll,

                },

                "graphics": {

                    "completed_lap": graphics.completed_lap,

                    "position": graphics.position,

                    "current_sector_index": graphics.current_sector_index,

                    "is_in_pit": graphics.is_in_pit,

                    "is_in_pit_lane": graphics.is_in_pit_lane,

                    "is_valid_lap": graphics.is_valid_lap,

                    "normalized_car_position": graphics.normalized_car_position,

                    "player_car_id": graphics.player_car_id,

                    "status": format!("{}", graphics.status),

                },

                "statics": {

                    "track": statics.track,

                    "car_model": statics.car_model,

                },

            }),

        })

    }

}



fn player_position(graphics: &GraphicsMap) -> [f32; 3] {

    let player_id = graphics.player_car_id;

    for (index, car_id) in graphics.car_id.iter().enumerate() {

        if *car_id == player_id {

            if let Some(position) = graphics.car_coordinates.get(index) {

                return [position.x, position.y, position.z];

            }

            break;

        }

    }



    graphics

        .car_coordinates

        .first()

        .map(|position| [position.x, position.y, position.z])

        .unwrap_or([0.0, 0.0, 0.0])

}



impl Drop for AccAdapter {

    fn drop(&mut self) {

        self.disconnect();

    }

}



#[cfg(not(target_os = "windows"))]

compile_error!("ACC adapter requires Windows");


