use crate::schema::{GameId, LapSummary, SessionInfo, SessionKind, TelemetrySample};

#[derive(Debug, Clone)]
pub enum AdapterEvent {
    None,
    SessionInfo(SessionInfo),
    Telemetry(TelemetrySample),
    LapStarted { lap_number: u32 },
    LapCompleted(LapSummary),
    /// The car cycled out through the pit lane / garage (e.g. ACC
    /// return-to-garage, or a normal pit stop) after completing at least one
    /// timed lap. The recorder rolls onto a fresh stint without measuring a
    /// physics-freeze break.
    StintBoundary,
    Heartbeat,
    Disconnected,
}

pub trait GameAdapter: Send {
    fn game_id(&self) -> GameId;
    fn poll(&mut self) -> AdapterEvent;
    fn is_active(&self) -> bool;

    /// Whether this adapter emits [`AdapterEvent::StintBoundary`] for pit-lane /
    /// garage cycles. When true, the recorder must **not** also split stints on
    /// a physics-freeze gap — with real pit detection that heuristic only
    /// manufactures phantom stints from alt-tabs, pause menus and sim hitches.
    fn detects_pit_stints(&self) -> bool {
        false
    }
}

/// Detects a pit-lane / garage cycle: the car went into the pits (or garage
/// stall) after completing at least one flying lap, then came back out.
/// Emitting an [`AdapterEvent::StintBoundary`] on that trailing edge lets the
/// recorder split a stint without leaning on a physics-freeze heuristic.
///
/// Each adapter feeds it whatever "in the pits" signal its sim exposes
/// (`is_in_pit_lane`, `mInPits`, `pit_status`, …) plus a per-lap "was this an
/// out/in-lap" flag.
#[derive(Debug, Default, Clone)]
pub struct PitCycleDetector {
    in_pits_prev: bool,
    visited: bool,
    flying_laps: u32,
}

impl PitCycleDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed lap. Out / in-laps (`pitted`) don't count toward the
    /// gate that a real run happened before a pit cycle can split the stint.
    pub fn lap_completed(&mut self, pitted: bool) {
        if !pitted {
            self.flying_laps = self.flying_laps.saturating_add(1);
        }
    }

    /// Feed the per-frame "car is in the pit lane / box / garage" flag. Returns
    /// true exactly once — on the frame the car leaves the pits again — when at
    /// least one flying lap has been completed since the last boundary.
    pub fn left_pits(&mut self, in_pits: bool) -> bool {
        if in_pits {
            self.visited = true;
        }
        let left = self.in_pits_prev && !in_pits;
        self.in_pits_prev = in_pits;
        if left && self.visited && self.flying_laps > 0 {
            self.visited = false;
            self.flying_laps = 0;
            true
        } else {
            false
        }
    }

    /// Clear all state on a real session / track / car change so a stale pit
    /// visit or lap tally can't leak into the next session.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
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

/// True when incoming telemetry reports a different, known session type than the
/// open session — e.g. ACC moving Practice → Qualifying on the same track and
/// car. Both sides must be a known kind; `Unknown` on either side never triggers
/// (a sim that reports no session type must not spawn a session per poll).
pub fn session_kind_changed(current: &SessionInfo, incoming: &SessionInfo) -> bool {
    current.session_kind != SessionKind::Unknown
        && incoming.session_kind != SessionKind::Unknown
        && current.session_kind != incoming.session_kind
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
            session_kind: crate::schema::SessionKind::Unknown,
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

    fn with_kind(kind: crate::schema::SessionKind) -> crate::schema::SessionInfo {
        crate::schema::SessionInfo {
            session_kind: kind,
            ..info("monza", "Monza")
        }
    }

    #[test]
    fn detects_session_kind_change() {
        use crate::schema::SessionKind;
        assert!(super::session_kind_changed(
            &with_kind(SessionKind::Practice),
            &with_kind(SessionKind::Qualifying),
        ));
        assert!(!super::session_kind_changed(
            &with_kind(SessionKind::Race),
            &with_kind(SessionKind::Race),
        ));
    }

    #[test]
    fn session_kind_change_needs_both_sides_known() {
        use crate::schema::SessionKind;
        assert!(!super::session_kind_changed(
            &with_kind(SessionKind::Unknown),
            &with_kind(SessionKind::Race),
        ));
        assert!(!super::session_kind_changed(
            &with_kind(SessionKind::Practice),
            &with_kind(SessionKind::Unknown),
        ));
    }

    #[test]
    fn pit_cycle_fires_once_on_leaving_after_a_flying_lap() {
        let mut d = super::PitCycleDetector::new();
        d.lap_completed(false); // one flying lap done
        assert!(!d.left_pits(true), "entering the pits is not the edge");
        assert!(!d.left_pits(true), "still in the pits");
        assert!(d.left_pits(false), "leaving the pits splits the stint");
        assert!(!d.left_pits(false), "only once per cycle");
        assert!(!d.left_pits(true), "next visit re-arms");
        assert!(!d.left_pits(false), "no flying lap since the last split");
    }

    #[test]
    fn pit_cycle_needs_a_flying_lap_first() {
        let mut d = super::PitCycleDetector::new();
        // Straight out of the garage for the first out-lap.
        assert!(!d.left_pits(true));
        assert!(!d.left_pits(false), "session's first pit-out is not a split");
    }

    #[test]
    fn pit_cycle_ignores_out_and_in_laps_for_the_gate() {
        let mut d = super::PitCycleDetector::new();
        d.lap_completed(true); // out-lap only, no real running
        assert!(!d.left_pits(true));
        assert!(!d.left_pits(false), "an out-lap alone doesn't arm a split");
        d.lap_completed(false); // now a genuine flying lap
        assert!(!d.left_pits(true));
        assert!(d.left_pits(false));
    }
}
