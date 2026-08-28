use crate::acc_statics::{AccStaticsSnapshot, STATIC_NAME, STATIC_SIZE};

use acc_shared_memory_rs::core::SharedMemoryReader;

use acc_shared_memory_rs::maps::GraphicsMap;

use acc_shared_memory_rs::parsers::{parse_graphics_map, parse_physics_map};

use chrono::Utc;

use sim_capture_common::SharedMemoryMapping;

use sim_core::{

    kmh_to_mps, normalize_brake, normalize_throttle, AdapterEvent,

    GameAdapter, GameId, LapSummary, SectorTimes, SessionInfo, TelemetrySample,

};



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

    current_lap_valid: bool,

    current_lap_in_pit: bool,

    lap_start_compound: Option<String>,

    lap_start_tc: Option<i32>,

    lap_start_abs: Option<i32>,

    lap_start_meta_captured: bool,

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

            current_lap_valid: true,

            current_lap_in_pit: false,

            lap_start_compound: None,

            lap_start_tc: None,

            lap_start_abs: None,

            lap_start_meta_captured: false,

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

        self.last_completed_laps = -1;

        self.last_physics_packet = -1;

        self.last_sector_index = -1;

        self.current_lap_valid = true;

        self.current_lap_in_pit = false;

        self.lap_start_compound = None;

        self.lap_start_tc = None;

        self.lap_start_abs = None;

        self.lap_start_meta_captured = false;

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



    fn track_lap_state(&mut self, graphics: &GraphicsMap) {

        if !graphics.is_valid_lap {

            self.current_lap_valid = false;

        }

        if graphics.is_in_pit_lane || graphics.is_in_pit {

            self.current_lap_in_pit = true;

        }

    }



    fn lap_completed_event(&mut self, graphics: &GraphicsMap) -> Option<AdapterEvent> {

        if self.last_completed_laps < 0 || graphics.completed_lap <= self.last_completed_laps {

            return None;

        }



        let lap_number = self.last_completed_laps + 1;

        let lap_time_ms = graphics.last_time.max(0) as u32;

        let valid = lap_time_ms > 0

            && self.current_lap_valid

            && !self.current_lap_in_pit;

        let sectors = sim_core::acc_cumulative_splits_to_sectors(
            self.sector_times.s1_ms,
            self.sector_times.s2_ms,
            lap_time_ms,
        );

        let summary = LapSummary {

            lap_number: lap_number as u32,

            lap_time_ms,

            valid,

            sectors,

            tyre_compound: self.lap_start_compound.take(),

            tc_level: self.lap_start_tc.take(),

            abs_level: self.lap_start_abs.take(),

            fuel_used_l: None,

        };



        self.lap_start_meta_captured = false;

        self.sector_times = SectorTimes {

            s1_ms: None,

            s2_ms: None,

            s3_ms: None,

        };

        self.last_completed_laps = graphics.completed_lap;

        self.last_sector_index = graphics.current_sector_index;

        self.current_lap_valid = true;

        self.current_lap_in_pit = false;



        Some(AdapterEvent::LapCompleted(summary))

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

        self.sector_times = SectorTimes {

            s1_ms: None,

            s2_ms: None,

            s3_ms: None,

        };

    }



    fn session_info(statics: &AccStaticsSnapshot) -> Option<SessionInfo> {

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

        })

    }



    fn session_ready(graphics: &GraphicsMap, statics: &AccStaticsSnapshot) -> bool {

        graphics.status.is_active() && Self::session_info(statics).is_some()

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

            self.last_completed_laps = graphics.completed_lap;

            self.last_sector_index = graphics.current_sector_index;

            if let Ok(physics) = parse_physics_map(self.physics_reader.as_ref().unwrap()) {

                self.last_physics_packet = physics.packet_id;

            }

            let info = Self::session_info(&statics).expect("session_ready guarantees session info");

            self.last_track_id = info.track_id.clone();

            return AdapterEvent::SessionInfo(info);

        }



        if let Some(info) = Self::session_info(&statics) {

            if !self.last_track_id.is_empty()

                && !info.track_id.trim().is_empty()

                && !self.last_track_id.eq_ignore_ascii_case(&info.track_id)

            {

                self.reset_lap_progress();

                self.last_track_id = info.track_id.clone();

                return AdapterEvent::SessionInfo(info);

            }

        }



        if !Self::session_ready(&graphics, &statics) {

            self.reset_lap_progress();

            return AdapterEvent::Heartbeat;

        }



        if self.last_completed_laps < 0 {

            self.last_completed_laps = graphics.completed_lap;

            self.last_sector_index = graphics.current_sector_index;

            if let Ok(physics) = parse_physics_map(self.physics_reader.as_ref().unwrap()) {

                self.last_physics_packet = physics.packet_id;

            }

        }



        self.update_sector_times(&graphics);

        self.track_lap_state(&graphics);

        self.capture_lap_start_meta(&graphics);



        if let Some(event) = self.lap_completed_event(&graphics) {

            return event;

        }



        let physics = match parse_physics_map(self.physics_reader.as_ref().unwrap()) {

            Ok(physics) => physics,

            Err(_) => {

                self.disconnect();

                return AdapterEvent::Disconnected;

            }

        };



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


