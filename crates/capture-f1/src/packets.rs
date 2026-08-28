pub const DEFAULT_PORT: u16 = 20777;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct PacketHeader {
    pub packet_format: u16,
    pub game_year: u8,
    pub game_major_version: u8,
    pub game_minor_version: u8,
    pub packet_version: u8,
    pub packet_id: u8,
    pub session_uid: u64,
    pub session_time: f32,
    pub frame_identifier: u32,
    pub overall_frame_identifier: u32,
    pub player_car_index: u8,
    pub secondary_player_car_index: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct CarTelemetryData {
    pub speed: u16,
    pub throttle: f32,
    pub steer: f32,
    pub brake: f32,
    pub clutch: u32,
    pub gear: i8,
    pub engine_rpm: u16,
    pub drs: u8,
    pub rev_lights_percent: u8,
    pub rev_lights_bit_value: u16,
    pub brakes_temperature: [u16; 4],
    pub tyres_surface_temperature: [u8; 4],
    pub tyres_inner_temperature: [u8; 4],
    pub engine_temperature: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LapData {
    pub last_lap_time_ms: u32,
    pub current_lap_time_ms: u32,
    pub sector1_time_ms_part: u16,
    pub sector1_time_minutes_part: u8,
    pub sector2_time_ms_part: u16,
    pub sector2_time_minutes_part: u8,
    pub delta_to_car_in_front_ms_part: u16,
    pub delta_to_car_in_front_minutes_part: u8,
    pub delta_to_race_leader_ms_part: u16,
    pub delta_to_race_leader_minutes_part: u8,
    pub lap_distance: f32,
    pub total_distance: f32,
    pub safety_car_delta: f32,
    pub car_position: u8,
    pub current_lap_num: u8,
    pub pit_status: u8,
    pub num_pit_stops: u8,
    pub sector: u8,
    pub current_lap_invalid: u8,
    pub penalties: u8,
    pub total_warnings: u8,
    pub corner_cutting_warnings: u8,
    pub num_unserved_drive_through_pens: u8,
    pub num_unserved_stop_go_pens: u8,
    pub grid_position: u8,
    pub driver_status: u8,
    pub result_status: u8,
    pub pit_lane_timer_active: u8,
    pub pit_lane_time_in_lane_ms: u16,
    pub pit_stop_timer_ms: u16,
    pub pit_stop_should_serve_pen: u8,
    pub speed_trap_fastest_speed: f32,
    pub speed_trap_fastest_lap: u8,
}

pub const PACKET_ID_MOTION: u8 = 0;
pub const PACKET_ID_SESSION: u8 = 1;
pub const PACKET_ID_LAP_DATA: u8 = 2;
pub const PACKET_ID_TELEMETRY: u8 = 6;

/// Byte offset of `trackId` (int8) inside a `PacketSessionData`, measured from
/// the start of the packet: header + weather(1) + trackTemp(1) + airTemp(1) +
/// totalLaps(1) + trackLength(2) + sessionType(1). Stable across F1 23–25.
pub const SESSION_TRACK_ID_OFFSET: usize = 7;

pub fn sector_ms(ms_part: u16, min_part: u8) -> u32 {
    ms_part as u32 + (min_part as u32) * 60_000
}
