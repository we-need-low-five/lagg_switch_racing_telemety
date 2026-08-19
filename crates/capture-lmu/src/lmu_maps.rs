use sim_capture_common::utf16_to_string;

pub const LMU_DATA_NAME: &str = "Local\\LMU_Data";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LmuTelemetry {
    pub active: i32,
    pub version: i32,
    pub session_time: f64,
    pub current_time: f64,
    pub lap_number: i32,
    pub lap_start_time: f64,
    pub track_name: [u16; 64],
    pub vehicle_name: [u16; 64],
    pub speed: f64,
    pub throttle: f64,
    pub brake: f64,
    pub clutch: f64,
    pub steering: f64,
    pub gear: i32,
    pub engine_rpm: f64,
    pub pos_x: f64,
    pub pos_y: f64,
    pub pos_z: f64,
    pub last_lap_time: f64,
    pub best_lap_time: f64,
    pub sector: i32,
    pub sector_time: f64,
    pub in_pits: i32,
    pub player_name: [u16; 32],
}

impl LmuTelemetry {
    pub fn track(&self) -> String {
        utf16_to_string(&self.track_name)
    }

    pub fn vehicle(&self) -> String {
        utf16_to_string(&self.vehicle_name)
    }

    pub fn player(&self) -> String {
        utf16_to_string(&self.player_name)
    }
}
