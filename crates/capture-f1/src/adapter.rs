use crate::packets::{
    sector_ms, CarTelemetryData, LapData, PacketHeader, DEFAULT_PORT, PACKET_ID_LAP_DATA,
    PACKET_ID_MOTION, PACKET_ID_TELEMETRY,
};
use chrono::Utc;
use sim_core::{
    normalize_brake, normalize_steering, normalize_throttle, AdapterEvent, GameAdapter, GameId,
    LapSummary, SectorTimes, SessionInfo, TelemetrySample,
};
use socket2::{Domain, Socket, Type};
use std::mem::{size_of, MaybeUninit};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct CarMotionData {
    world_position_x: f32,
    world_position_y: f32,
    world_position_z: f32,
    world_velocity_x: f32,
    world_velocity_y: f32,
    world_velocity_z: f32,
}

pub struct F1Adapter {
    socket: Option<Socket>,
    player_index: u8,
    last_lap_num: u8,
    /// Latched from the lap that just finished (not the new current lap).
    completed_lap_invalid: u8,
    session_announced: bool,
    latest_lap: Option<LapData>,
    latest_telemetry: Option<CarTelemetryData>,
    latest_motion: Option<(f32, f32, f32)>,
    sector_times: SectorTimes,
    port: u16,
}

impl F1Adapter {
    pub fn new() -> Self {
        Self::with_port(DEFAULT_PORT)
    }

    pub fn with_port(port: u16) -> Self {
        Self {
            socket: None,
            player_index: 0,
            last_lap_num: 0,
            completed_lap_invalid: 0,
            session_announced: false,
            latest_lap: None,
            latest_telemetry: None,
            latest_motion: None,
            sector_times: SectorTimes {
                s1_ms: None,
                s2_ms: None,
                s3_ms: None,
            },
            port,
        }
    }

    fn bind(&mut self) -> bool {
        if self.socket.is_some() {
            return true;
        }
        let socket = match Socket::new(Domain::IPV4, Type::DGRAM, None) {
            Ok(s) => s,
            Err(_) => return false,
        };
        // Bind localhost only — avoid accepting spoofed telemetry from the LAN.
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        if socket.set_reuse_address(true).is_err() {
            return false;
        }
        if socket.bind(&addr.into()).is_err() {
            return false;
        }
        let _ = socket.set_read_timeout(Some(Duration::from_millis(1)));
        self.socket = Some(socket);
        true
    }

    fn read_packets(&mut self) {
        let mut buf = [MaybeUninit::<u8>::uninit(); 2048];
        loop {
            let received = self
                .socket
                .as_ref()
                .and_then(|socket| socket.recv(&mut buf).ok());
            let Some(len) = received else {
                break;
            };
            if len < size_of::<PacketHeader>() {
                continue;
            }
            let slice = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, len) };
            let header = read_unaligned::<PacketHeader>(slice);
            self.player_index = header.player_car_index;
            match header.packet_id {
                PACKET_ID_MOTION => self.parse_motion(slice, len),
                PACKET_ID_LAP_DATA => self.parse_lap_data(slice, len),
                PACKET_ID_TELEMETRY => self.parse_telemetry(slice, len),
                _ => {}
            }
        }
    }

    fn parse_motion(&mut self, buf: &[u8], len: usize) {
        let item_size = size_of::<CarMotionData>();
        let offset = size_of::<PacketHeader>();
        let idx = self.player_index as usize;
        let start = offset + idx * item_size;
        if start + item_size <= len {
            let motion = read_unaligned::<CarMotionData>(&buf[start..]);
            self.latest_motion = Some((
                motion.world_position_x,
                motion.world_position_y,
                motion.world_position_z,
            ));
        }
    }

    fn parse_lap_data(&mut self, buf: &[u8], len: usize) {
        let item_size = size_of::<LapData>();
        let offset = size_of::<PacketHeader>();
        let idx = self.player_index as usize;
        let start = offset + idx * item_size;
        if start + item_size <= len {
            let lap = read_unaligned::<LapData>(&buf[start..]);
            self.latest_lap = Some(lap);
        }
    }

    fn parse_telemetry(&mut self, buf: &[u8], len: usize) {
        let item_size = size_of::<CarTelemetryData>();
        let offset = size_of::<PacketHeader>();
        let idx = self.player_index as usize;
        let start = offset + idx * item_size;
        if start + item_size <= len {
            self.latest_telemetry = Some(read_unaligned::<CarTelemetryData>(&buf[start..]));
        }
    }

    fn latch_sector_times(&mut self, lap: &LapData) {
        let s1_ms = { lap.sector1_time_ms_part };
        let s1_min = { lap.sector1_time_minutes_part };
        let s2_ms = { lap.sector2_time_ms_part };
        let s2_min = { lap.sector2_time_minutes_part };
        let s1 = sector_ms(s1_ms, s1_min);
        let s2 = sector_ms(s2_ms, s2_min);
        if s1 > 0 {
            self.sector_times.s1_ms = Some(s1);
        }
        if s2 > 0 {
            self.sector_times.s2_ms = Some(s2);
        }
    }
}

impl Default for F1Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GameAdapter for F1Adapter {
    fn game_id(&self) -> GameId {
        GameId::F1_25
    }

    fn is_active(&self) -> bool {
        self.latest_lap.is_some()
    }

    fn poll(&mut self) -> AdapterEvent {
        if !self.bind() {
            return AdapterEvent::Disconnected;
        }
        self.read_packets();

        let Some(lap) = self.latest_lap else {
            return AdapterEvent::Disconnected;
        };

        if !self.session_announced {
            self.session_announced = true;
            return AdapterEvent::SessionInfo(SessionInfo {
                game: GameId::F1_25,
                track_id: String::new(),
                track: "F1 25 Session".to_string(),
                car: "Player Car".to_string(),
                game_version: "F1 25".to_string(),
                player_name: "Player".to_string(),
            });
        }

        if self.last_lap_num > 0 && lap.current_lap_num > self.last_lap_num {
            let lap_time_ms = lap.last_lap_time_ms;
            // Use validity latched from the finished lap, not the new current lap.
            let valid = self.completed_lap_invalid == 0 && lap_time_ms > 0;
            let mut sectors = self.sector_times.clone();
            if let (Some(s1), Some(s2)) = (sectors.s1_ms, sectors.s2_ms) {
                if lap_time_ms > s1 + s2 {
                    sectors.s3_ms = Some(lap_time_ms - s1 - s2);
                }
            }
            let summary = LapSummary {
                lap_number: self.last_lap_num as u32,
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
            self.last_lap_num = lap.current_lap_num;
            self.completed_lap_invalid = lap.current_lap_invalid;
            self.latch_sector_times(&lap);
            return AdapterEvent::LapCompleted(summary);
        }

        if self.last_lap_num == 0 {
            self.last_lap_num = lap.current_lap_num.max(1);
            self.completed_lap_invalid = lap.current_lap_invalid;
            return AdapterEvent::LapStarted {
                lap_number: self.last_lap_num as u32,
            };
        }

        // Still on the same lap — latch invalid flag and sector splits for when it ends.
        self.completed_lap_invalid = lap.current_lap_invalid;
        self.latch_sector_times(&lap);
        self.last_lap_num = lap.current_lap_num;

        let (pos_x, pos_y, pos_z) = self.latest_motion.unwrap_or((0.0, 0.0, 0.0));
        let (speed, throttle, brake, steer, gear, rpm, tyre_temps) =
            if let Some(t) = self.latest_telemetry {
                (
                    t.speed as f32 / 3.6,
                    t.throttle,
                    t.brake,
                    t.steer,
                    t.gear as i32,
                    t.engine_rpm as f32,
                    Some(t.tyres_inner_temperature),
                )
            } else {
                (0.0, 0.0, 0.0, 0.0, 0, 0.0, None)
            };

        let pit_status = lap.pit_status;
        let sector = lap.sector;
        let total_distance = lap.total_distance;

        AdapterEvent::Telemetry(TelemetrySample {
            timestamp: Utc::now(),
            lap_time_s: lap.current_lap_time_ms as f32 / 1000.0,
            distance_m: lap.lap_distance,
            speed_mps: speed,
            throttle: normalize_throttle(throttle),
            brake: normalize_brake(brake),
            steering: normalize_steering(steer),
            gear,
            rpm,
            pos_x,
            pos_y,
            pos_z,
            fuel: None,
            tyre_temp_fl: tyre_temps.map(|t| t[0] as f32),
            tyre_temp_fr: tyre_temps.map(|t| t[1] as f32),
            tyre_temp_rl: tyre_temps.map(|t| t[2] as f32),
            tyre_temp_rr: tyre_temps.map(|t| t[3] as f32),
            tyre_press_fl: None,
            tyre_press_fr: None,
            tyre_press_rl: None,
            tyre_press_rr: None,
            raw: serde_json::json!({
                "pit_status": pit_status,
                "sector": sector,
                "total_distance": total_distance,
            }),
        })
    }
}

fn read_unaligned<T: Copy>(bytes: &[u8]) -> T {
    let mut value = MaybeUninit::<T>::uninit();
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            value.as_mut_ptr() as *mut u8,
            size_of::<T>(),
        );
        value.assume_init()
    }
}
