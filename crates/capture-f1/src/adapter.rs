use crate::packets::{
    sector_ms, CarTelemetryData, LapData, PacketHeader, DEFAULT_PORT, PACKET_ID_LAP_DATA,
    PACKET_ID_MOTION, PACKET_ID_SESSION, PACKET_ID_TELEMETRY, SESSION_TRACK_ID_OFFSET,
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
    /// Slug of the announced track, for detecting a mid-session track change.
    last_track_id: String,
    /// `(slug, display)` from the most recent session packet, if a known id.
    parsed_track: Option<(String, String)>,
    latest_lap: Option<LapData>,
    latest_telemetry: Option<CarTelemetryData>,
    latest_motion: Option<(f32, f32, f32)>,
    sector_times: SectorTimes,
    port: u16,
}

/// F1 25 UDP `trackId` → (slug, display name). Unknown / `-1` ids fall back to
/// the generic label so recording still works.
const F1_TRACKS: &[(i8, &str, &str)] = &[
    (0, "melbourne", "Melbourne"),
    (1, "paul_ricard", "Paul Ricard"),
    (2, "shanghai", "Shanghai"),
    (3, "sakhir", "Sakhir (Bahrain)"),
    (4, "catalunya", "Catalunya"),
    (5, "monaco", "Monaco"),
    (6, "montreal", "Montreal"),
    (7, "silverstone", "Silverstone"),
    (8, "hockenheim", "Hockenheim"),
    (9, "hungaroring", "Hungaroring"),
    (10, "spa", "Spa-Francorchamps"),
    (11, "monza", "Monza"),
    (12, "singapore", "Singapore"),
    (13, "suzuka", "Suzuka"),
    (14, "abu_dhabi", "Abu Dhabi"),
    (15, "cota", "Circuit of the Americas"),
    (16, "interlagos", "Interlagos"),
    (17, "red_bull_ring", "Red Bull Ring"),
    (18, "sochi", "Sochi"),
    (19, "mexico", "Mexico City"),
    (20, "baku", "Baku"),
    (21, "sakhir_short", "Sakhir Short"),
    (22, "silverstone_short", "Silverstone Short"),
    (23, "cota_short", "COTA Short"),
    (24, "suzuka_short", "Suzuka Short"),
    (25, "hanoi", "Hanoi"),
    (26, "zandvoort", "Zandvoort"),
    (27, "imola", "Imola"),
    (28, "portimao", "Portimão"),
    (29, "jeddah", "Jeddah"),
    (30, "miami", "Miami"),
    (31, "las_vegas", "Las Vegas"),
    (32, "losail", "Losail (Qatar)"),
    (33, "silverstone_reverse", "Silverstone (Reverse)"),
    (34, "red_bull_ring_reverse", "Red Bull Ring (Reverse)"),
    (35, "zandvoort_reverse", "Zandvoort (Reverse)"),
];

fn f1_track(track_id: i8) -> Option<(&'static str, &'static str)> {
    F1_TRACKS
        .iter()
        .find(|(id, _, _)| *id == track_id)
        .map(|(_, slug, display)| (*slug, *display))
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
            last_track_id: String::new(),
            parsed_track: None,
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
                PACKET_ID_SESSION => self.parse_session(slice, len),
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

    fn parse_session(&mut self, buf: &[u8], len: usize) {
        let offset = size_of::<PacketHeader>() + SESSION_TRACK_ID_OFFSET;
        if offset >= len {
            return;
        }
        let track_id = buf[offset] as i8;
        if let Some((slug, display)) = f1_track(track_id) {
            self.parsed_track = Some((slug.to_string(), display.to_string()));
        }
        // Unknown / -1 id: keep whatever we had (falls back to the placeholder).
    }

    /// `(track_id slug, display name)` — empty slug + generic name until a
    /// session packet with a known track id has been seen.
    fn current_track(&self) -> (String, String) {
        self.parsed_track
            .clone()
            .unwrap_or_else(|| (String::new(), "F1 25 Session".to_string()))
    }

    fn session_info(track_id: String, track: String) -> SessionInfo {
        SessionInfo {
            game: GameId::F1_25,
            track_id,
            track,
            car: "Player Car".to_string(),
            game_version: "F1 25".to_string(),
            player_name: "Player".to_string(),
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

        let (track_id, track_name) = self.current_track();

        if !self.session_announced {
            self.session_announced = true;
            self.last_track_id = track_id.clone();
            return AdapterEvent::SessionInfo(Self::session_info(track_id, track_name));
        }

        // Re-announce once the real track becomes known (announced with the
        // placeholder), or when the player switches track mid-run. The stale
        // placeholder session, if any, is lapless and gets pruned.
        if !track_id.is_empty() && !self.last_track_id.eq_ignore_ascii_case(&track_id) {
            self.last_track_id = track_id.clone();
            self.last_lap_num = 0;
            self.completed_lap_invalid = 0;
            self.sector_times = SectorTimes {
                s1_ms: None,
                s2_ms: None,
                s3_ms: None,
            };
            return AdapterEvent::SessionInfo(Self::session_info(track_id, track_name));
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
            g_force_x: None,
            g_force_y: None,
            g_force_z: None,
            slip_angle_fl: None,
            slip_angle_fr: None,
            slip_angle_rl: None,
            slip_angle_rr: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn session_packet(track_id: i8) -> Vec<u8> {
        let mut buf = vec![0u8; size_of::<PacketHeader>() + SESSION_TRACK_ID_OFFSET + 1];
        let last = buf.len() - 1;
        buf[last] = track_id as u8;
        buf
    }

    #[test]
    fn f1_track_lookup_known_and_unknown() {
        assert_eq!(f1_track(11), Some(("monza", "Monza")));
        assert_eq!(f1_track(0), Some(("melbourne", "Melbourne")));
        assert_eq!(f1_track(-1), None);
        assert_eq!(f1_track(120), None);
    }

    #[test]
    fn parse_session_tracks_and_updates_the_circuit() {
        let mut a = F1Adapter::new();
        assert_eq!(a.current_track(), (String::new(), "F1 25 Session".into()));

        let monza = session_packet(11);
        a.parse_session(&monza, monza.len());
        assert_eq!(a.current_track(), ("monza".into(), "Monza".into()));

        let spa = session_packet(10);
        a.parse_session(&spa, spa.len());
        assert_eq!(a.current_track(), ("spa".into(), "Spa-Francorchamps".into()));

        // Unknown id keeps the last known circuit rather than dropping to placeholder.
        let unknown = session_packet(-1);
        a.parse_session(&unknown, unknown.len());
        assert_eq!(a.current_track(), ("spa".into(), "Spa-Francorchamps".into()));
    }

    #[test]
    fn parse_session_ignores_a_truncated_packet() {
        let mut a = F1Adapter::new();
        let short = vec![0u8; size_of::<PacketHeader>()];
        a.parse_session(&short, short.len());
        assert_eq!(a.current_track(), (String::new(), "F1 25 Session".into()));
    }
}
