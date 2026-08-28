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

/// True when incoming telemetry is a different circuit than the open session.
pub fn session_track_changed(current: &SessionInfo, incoming: &SessionInfo) -> bool {
    let current_id = current.track_id.trim();
    let incoming_id = incoming.track_id.trim();
    if !current_id.is_empty() && !incoming_id.is_empty() {
        return !current_id.eq_ignore_ascii_case(incoming_id);
    }
    let current_name = current.track.trim();
    let incoming_name = incoming.track.trim();
    !current_name.is_empty()
        && !incoming_name.is_empty()
        && !current_name.eq_ignore_ascii_case(incoming_name)
}

/// True when incoming telemetry reports a different car than the open session.
/// Both sides must be known — a blank car never triggers a new session.
pub fn session_car_changed(current: &SessionInfo, incoming: &SessionInfo) -> bool {
    let a = current.car.trim();
    let b = incoming.car.trim();
    !a.is_empty() && !b.is_empty() && !a.eq_ignore_ascii_case(b)
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

    fn info(track_id: &str, track: &str) -> crate::schema::SessionInfo {
        crate::schema::SessionInfo {
            game: crate::schema::GameId::Acc,
            track_id: track_id.to_string(),
            track: track.to_string(),
            car: "Ferrari".into(),
            game_version: "1".into(),
            player_name: "P".into(),
        }
    }

    #[test]
    fn detects_track_id_change() {
        assert!(super::session_track_changed(
            &info("monza", "Monza"),
            &info("spa", "Spa"),
        ));
        assert!(!super::session_track_changed(
            &info("monza", "Monza"),
            &info("MONZA", "Monza"),
        ));
    }

    #[test]
    fn falls_back_to_track_name_when_ids_empty() {
        assert!(super::session_track_changed(
            &info("", "Monza"),
            &info("", "Spa"),
        ));
        assert!(super::session_track_changed(
            &info("monza", "Monza"),
            &info("", "Spa"),
        ));
        assert!(!super::session_track_changed(
            &info("monza", "Monza"),
            &info("", "Monza"),
        ));
    }

    fn with_car(car: &str) -> crate::schema::SessionInfo {
        crate::schema::SessionInfo {
            car: car.to_string(),
            ..info("monza", "Monza")
        }
    }

    #[test]
    fn detects_car_change() {
        assert!(super::session_car_changed(
            &with_car("Ferrari 296 GT3"),
            &with_car("Porsche 992 GT3 R"),
        ));
        assert!(!super::session_car_changed(
            &with_car("Ferrari 296 GT3"),
            &with_car("ferrari 296 gt3"),
        ));
    }

    #[test]
    fn car_change_needs_both_sides_known() {
        assert!(!super::session_car_changed(&with_car(""), &with_car("Porsche")));
        assert!(!super::session_car_changed(&with_car("Ferrari"), &with_car("  ")));
    }
}
