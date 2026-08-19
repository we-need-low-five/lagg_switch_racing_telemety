use sim_capture_common::utf16_to_string;

pub const PHYSICS_NAME: &str = "Local\\acpmf_physics";
pub const GRAPHICS_NAME: &str = "Local\\acpmf_graphics";
pub const STATIC_NAME: &str = "Local\\acpmf_static";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AcPhysics {
    pub packet_id: i32,
    pub gas: f32,
    pub brake: f32,
    pub fuel: f32,
    pub gear: i32,
    pub rpm: i32,
    pub steer_angle: f32,
    pub speed_kmh: f32,
    pub velocity: [f32; 3],
    pub acc_g: [f32; 3],
    pub wheel_slip: [f32; 4],
    pub wheel_load: [f32; 4],
    pub wheels_pressure: [f32; 4],
    pub wheel_angular_speed: [f32; 4],
    pub tyre_wear: [f32; 4],
    pub tyre_dirty_level: [f32; 4],
    pub tyre_core_temperature: [f32; 4],
    pub camber_rad: [f32; 4],
    pub suspension_travel: [f32; 4],
    pub drs: f32,
    pub tc: f32,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    pub cg_height: f32,
    pub car_damage: [f32; 5],
    pub number_of_tyres_out: i32,
    pub pit_limiter_on: i32,
    pub abs: f32,
    pub kers_charge: f32,
    pub kers_input: f32,
    pub auto_shifter_on: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AcGraphics {
    pub packet_id: i32,
    pub status: i32,
    pub session: i32,
    pub current_time: [u16; 15],
    pub last_time: [u16; 15],
    pub best_time: [u16; 15],
    pub split: [u16; 15],
    pub completed_laps: i32,
    pub position: i32,
    pub i_current_time: i32,
    pub i_last_time: i32,
    pub i_best_time: i32,
    pub session_time_left: f32,
    pub distance_traveled: f32,
    pub is_in_pit: i32,
    pub current_sector_index: i32,
    pub last_sector_time: i32,
    pub number_of_laps: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AcStatics {
    pub sm_version: [u16; 15],
    pub ac_version: [u16; 15],
    pub number_of_sessions: i32,
    pub num_cars: i32,
    pub car_model: [u16; 33],
    pub track: [u16; 33],
    pub player_name: [u16; 33],
    pub player_surname: [u16; 33],
    pub player_nick: [u16; 33],
    pub sector_count: i32,
    pub max_rpm: i32,
    pub max_fuel: f32,
}

impl AcStatics {
    pub fn track_name(&self) -> String {
        utf16_to_string(&self.track)
    }

    pub fn car_name(&self) -> String {
        utf16_to_string(&self.car_model)
    }

    pub fn player_display(&self) -> String {
        utf16_to_string(&self.player_nick)
    }

    pub fn game_version(&self) -> String {
        utf16_to_string(&self.ac_version)
    }
}
