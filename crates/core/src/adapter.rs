use crate::schema::{GameId, LapSummary, SessionInfo, TelemetrySample};

#[derive(Debug, Clone)]
pub enum AdapterEvent {
    None,
    SessionInfo(SessionInfo),
    Telemetry(TelemetrySample),
    LapStarted { lap_number: u32 },
    LapCompleted(LapSummary),
    Heartbeat,
    Disconnected,
}

pub trait GameAdapter: Send {
    fn game_id(&self) -> GameId;
    fn poll(&mut self) -> AdapterEvent;
    fn is_active(&self) -> bool;
}

pub fn normalize_throttle(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub fn normalize_brake(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

pub fn normalize_steering(value: f32) -> f32 {
    value.clamp(-1.0, 1.0)
}

pub fn kmh_to_mps(kmh: f32) -> f32 {
    kmh / 3.6
}
