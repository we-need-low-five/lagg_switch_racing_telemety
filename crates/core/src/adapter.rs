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

/// Hold the last engaged gear through a 1-sample (or short) drop to N/R.
/// ACC/AC report 0=R, 1=N, 2=1st, so shifts spike to 1. Sequential ±1 changes
/// (including F1 2→1) are kept; skip-shifts (|Δ|>1 into a real gear) are kept.
pub fn hold_transient_gear(held: i32, current: i32) -> i32 {
    if (current - held).abs() <= 1 {
        current
    } else if current <= 1 && held >= 2 {
        held
    } else {
        current
    }
}

pub fn kmh_to_mps(kmh: f32) -> f32 {
    kmh / 3.6
}

#[cfg(test)]
mod tests {
    use super::hold_transient_gear;

    #[test]
    fn holds_acc_neutral_blip() {
        assert_eq!(hold_transient_gear(4, 1), 4);
        assert_eq!(hold_transient_gear(5, 1), 5);
        assert_eq!(hold_transient_gear(3, 0), 3);
    }

    #[test]
    fn keeps_sequential_shifts() {
        assert_eq!(hold_transient_gear(4, 5), 5);
        assert_eq!(hold_transient_gear(4, 3), 3);
        assert_eq!(hold_transient_gear(2, 1), 1);
    }

    #[test]
    fn keeps_skip_upshift() {
        assert_eq!(hold_transient_gear(4, 6), 6);
    }
}
