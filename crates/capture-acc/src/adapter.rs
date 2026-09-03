use crate::acc_statics::{AccStaticsSnapshot, STATIC_NAME, STATIC_SIZE};

use acc_shared_memory_rs::core::SharedMemoryReader;

use acc_shared_memory_rs::enums::AccSessionType;

use acc_shared_memory_rs::maps::GraphicsMap;

use acc_shared_memory_rs::parsers::{parse_graphics_map, parse_physics_map};

use chrono::Utc;

use sim_capture_common::SharedMemoryMapping;

use sim_core::{

    kmh_to_mps, normalize_brake, normalize_throttle, AdapterEvent,

    GameAdapter, GameId, LapSummary, PitCycleDetector, SectorTimes, SessionInfo, SessionKind,
    TelemetrySample,

};

/// Map ACC's session type onto the sim-agnostic [`SessionKind`].
fn acc_session_kind(session_type: AccSessionType) -> SessionKind {
    match session_type {
        AccSessionType::Practice => SessionKind::Practice,
        AccSessionType::Qualifying | AccSessionType::HotlapSuperpole => SessionKind::Qualifying,
        AccSessionType::Race => SessionKind::Race,
        AccSessionType::Hotlap => SessionKind::Hotlap,
        AccSessionType::TimeAttack => SessionKind::TimeAttack,
        AccSessionType::Drift | AccSessionType::Drag | AccSessionType::Hotstint => SessionKind::Other,
        AccSessionType::Unknown => SessionKind::Unknown,
    }
}

/// Sentinel for "no session type observed yet" in the adapter's last-seen state.
const SESSION_UNSET: i32 = i32::MIN;

/// How long a taken crossing waits for ACC to score it before falling back to
/// the measured time, in polls (`RECORDER_POLL_INTERVAL` is 3 ms, so ~180 ms).
/// An out-lap ACC never scores always burns the whole window.
const SCORING_GRACE_POLLS: u32 = 60;

/// How far into a lap a boundary re-arms, so one crossing seen as both a wrap
/// and an ACC-scored lap cannot be taken twice.
///
/// This was a lap-fraction window — `normalized_car_position` between 0.20 and
/// 0.80 — which a lap covering less than a fifth of the spline never cleared.
/// On the Nurburgring 24h layout a lap of the GP loop alone is 4.6 km of a
/// 25.4 km spline, so the boundary stayed disarmed after the first one and the
/// next three crossings were dropped: their telemetry ran into the following
/// lap's recording, whose samples then spanned 44 km and four lap timers.
/// Elapsed time does not care how long the lap is.
const BOUNDARY_SETTLE_MS: i32 = 5_000;



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

    /// Sector whose split ACC still owes us: 0 = S1, 1 = S2. Armed when
    /// `current_sector_index` moves, cleared by the first tick that carries a
    /// usable `iLastSectorTime`.
    pending_sector_index: Option<i32>,

    session_announced: bool,

    last_track_id: String,

    last_car: String,

    /// ACC `graphics.session_type` / `session_index` at the last announce. ACC
    /// keeps the same track and car across Practice → Qualifying → Race but
    /// restarts `completed_lap` each phase, so a change here is a real session
    /// boundary. `SESSION_UNSET` until the first announce.
    last_session_type: i32,

    last_session_index: i32,

    current_lap_valid: bool,

    current_lap_in_pit: bool,

    lap_start_compound: Option<String>,

    lap_start_tc: Option<i32>,

    lap_start_abs: Option<i32>,

    lap_start_meta_captured: bool,

    /// Adapter-owned lap counter. ACC's `completed_lap` stalls or restarts
    /// across out-laps and return-to-garage, so lap numbers are minted here on
    /// each emitted lap instead. Reset only on a real session / phase change.
    lap_counter: u32,

    /// `normalized_car_position` on the previous poll (`-1.0` until first read).
    /// A high→low wrap is the start/finish line and drives the lap boundary
    /// even when `completed_lap` never moved (unscored out-laps).
    last_norm_pos: f32,

    /// Largest `current_time` (live lap timer, ms) seen since the last
    /// boundary. Fallback lap time when ACC's `last_time` is 0 / a sentinel.
    max_current_time_ms: u32,

    /// False while a just-taken boundary is still settling; blocks the
    /// `completed_lap` delta and the S/F wrap from firing twice for one lap.
    boundary_settled: bool,

    /// A `LapStarted` owed to the recorder from the last S/F crossing. ACC has
    /// no lap-start signal of its own, so the adapter synthesises one at every
    /// boundary — that's what clears the recorder's truncated-lap taint and
    /// sample buffer, exactly like AC / LMU / F1.
    pending_lap_started: Option<u32>,

    /// Crossing taken but not yet stamped with a lap time — see [`PendingLap`].
    pending_lap: Option<PendingLap>,

    /// Return-to-garage / pit-stop → new stint, off the `is_in_pit_lane` edge.
    pit_cycle: PitCycleDetector,

}

/// A start/finish crossing, held for the few polls ACC needs to score the lap.
///
/// ACC publishes `iLastTime` a moment *after* the car crosses the line, so on
/// the wrap tick it still holds the previous lap's time — and around a circuit
/// where consecutive laps are seconds apart that value sails through
/// `resolve_lap_time_ms`'s agreement band. Reading it there stamped every lap
/// with the one before it. Everything the finished lap needs is captured at the
/// crossing; only the time waits.
struct PendingLap {
    valid: bool,
    in_pit: bool,
    s1_ms: Option<u32>,
    s2_ms: Option<u32>,
    /// Peak of ACC's live lap timer over the lap — the fallback time for a
    /// crossing ACC never scores (garage out-laps).
    measured_ms: u32,
    tyre_compound: Option<String>,
    tc_level: Option<i32>,
    abs_level: Option<i32>,
    /// `iLastTime` / `completedLaps` as they read on the crossing tick. The lap
    /// is scored once either of them moves.
    acc_last_time: i32,
    completed_laps: i32,
    /// ACC scored the lap on the crossing tick itself (the boundary came from a
    /// `completedLaps` bump): `iLastTime` already describes it, nothing to wait
    /// for.
    scored_at_crossing: bool,
    polls: u32,
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

            pending_sector_index: None,

            session_announced: false,

            last_track_id: String::new(),

            last_car: String::new(),

            last_session_type: SESSION_UNSET,

            last_session_index: SESSION_UNSET,

            current_lap_valid: true,

            current_lap_in_pit: false,

            lap_start_compound: None,

            lap_start_tc: None,

            lap_start_abs: None,

            lap_start_meta_captured: false,

            lap_counter: 0,

            last_norm_pos: -1.0,

            max_current_time_ms: 0,

            boundary_settled: true,

            pending_lap_started: None,

            pending_lap: None,

            pit_cycle: PitCycleDetector::new(),

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

        self.last_car.clear();

        self.last_session_type = SESSION_UNSET;

        self.last_session_index = SESSION_UNSET;

        self.last_completed_laps = -1;

        self.last_physics_packet = -1;

        self.last_sector_index = -1;
        self.pending_sector_index = None;

        self.current_lap_valid = true;

        self.current_lap_in_pit = false;

        self.lap_start_compound = None;

        self.lap_start_tc = None;

        self.lap_start_abs = None;

        self.lap_start_meta_captured = false;

        self.lap_counter = 0;

        self.last_norm_pos = -1.0;

        self.max_current_time_ms = 0;

        self.boundary_settled = true;

        self.pending_lap_started = None;

        self.pending_lap = None;

        self.pit_cycle.reset();

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



    /// ACC current_sector_index is 0-based (0=S1, 1=S2, 2=S3).
    /// last_sector_time at S1/S2 lines is cumulative elapsed; S3 is derived at lap end.
    ///
    /// The index has to advance on its own, because ACC holds `iLastSectorTime`
    /// at 0 through the start of a lap. Gating the advance on a usable split
    /// stranded `last_sector_index` on S3 whenever the crossing was seen before
    /// ACC flipped the index to S1, and it stayed a sector behind for the rest of
    /// the lap: the S1 line was read as the lap-start flip and its split thrown
    /// away, then the S2 line filed cumulative-S2 as the S2 split with no S1 left
    /// to subtract. A Nordschleife lap came back as S1 blank, S2 5:33.
    fn update_sector_times(&mut self, sector_index: i32, last_sector_time: i32) {
        if sector_index != self.last_sector_index {
            // Leaving index 0 finishes S1, leaving index 1 finishes S2. Anything
            // else (S3, or the -1 sentinel) owes us nothing.
            self.pending_sector_index = match self.last_sector_index {
                index @ (0 | 1) => Some(index),
                _ => None,
            };
            self.last_sector_index = sector_index;
        }

        if last_sector_time <= 0 {
            return;
        }
        match self.pending_sector_index.take() {
            Some(0) => self.sector_times.s1_ms = Some(last_sector_time as u32),
            Some(1) => self.sector_times.s2_ms = Some(last_sector_time as u32),
            _ => {}
        }
    }



    fn track_lap_state(&mut self, graphics: &GraphicsMap, speed_kmh: f32) {

        // ACC keeps `is_valid_lap` at 0 whenever no flying lap is being timed —
        // stationary, in the pit lane, or in the instant after crossing the
        // line before the next lap's timing starts. A stopped car can't be
        // invalidating the lap it already finished, so only honour the flag
        // while the car is actually running a lap.
        if !graphics.is_valid_lap && speed_kmh > 5.0 && !graphics.is_in_pit_lane {

            self.current_lap_valid = false;

        }

        if graphics.is_in_pit_lane || graphics.is_in_pit {

            self.current_lap_in_pit = true;

        }

    }



    /// Fold ACC's lap counter back in when it moves *backwards*.
    ///
    /// ACC restarts `completedLaps` when it throws a lap away: a joker lap
    /// round the GP loop at the Nurburgring 24h is invalidated and both the
    /// counter and the lap timer reset. Of the places `poll` writes
    /// `last_completed_laps`, only the taken-boundary one can lower it, and
    /// the reset seldom lands on that exact tick — during the scoring grace
    /// window `poll` returns early through the `pending_lap` branch, so a drop
    /// seen there went unrecorded. The stale high value then holds
    /// `acc_scored` false until ACC climbs back past it, and every real lap in
    /// between falls back to the sampled live-timer peak instead of ACC's
    /// exact `iLastTime`.
    ///
    /// A counter that went down is a reset, never a scored lap. `-1` is the
    /// "no session read yet" sentinel and is left for `poll` to initialise.
    fn resync_completed_laps(&mut self, completed_lap: i32) {
        if self.last_completed_laps >= 0 && completed_lap < self.last_completed_laps {
            self.last_completed_laps = completed_lap;
        }
    }

    /// Take the lap that just ended at a start/finish boundary and park it in
    /// `pending_lap`. `poll` calls this once per crossing, whether the boundary
    /// was seen as an ACC `completed_lap` increment or as a
    /// `normalized_car_position` wrap; [`Self::score_pending_lap`] emits it.
    ///
    /// `lap_valid_snapshot` is `current_lap_valid` as of the previous tick — on
    /// the boundary tick ACC's live `is_valid_lap` already describes the next
    /// lap, so the finished lap is judged on the pre-tick value.
    ///
    /// Per-lap state is cleared here, at the crossing: the next lap's sectors
    /// and lap timer start now, whatever the finished lap ends up being worth.
    fn take_lap(&mut self, graphics: &GraphicsMap, lap_valid_snapshot: bool, scored: bool) {
        let in_pit = self.current_lap_in_pit;

        self.pending_lap = Some(PendingLap {
            valid: lap_valid_snapshot && !in_pit,
            in_pit,
            s1_ms: self.sector_times.s1_ms,
            s2_ms: self.sector_times.s2_ms,
            measured_ms: self.max_current_time_ms,
            tyre_compound: self.lap_start_compound.take(),
            tc_level: self.lap_start_tc.take(),
            abs_level: self.lap_start_abs.take(),
            acc_last_time: graphics.last_time,
            completed_laps: graphics.completed_lap,
            scored_at_crossing: scored,
            polls: 0,
        });

        self.lap_start_meta_captured = false;
        self.sector_times = SectorTimes {
            s1_ms: None,
            s2_ms: None,
            s3_ms: None,
        };
        self.last_sector_index = graphics.current_sector_index;
        self.pending_sector_index = None;
        self.current_lap_valid = true;
        self.current_lap_in_pit = false;
        self.max_current_time_ms = 0;
    }

    /// Emit the held crossing once ACC has scored it — `iLastTime` or
    /// `completedLaps` moved — or once the grace window runs out and the
    /// measured time has to do.
    ///
    /// `None` means "still waiting", or that the scored lap had no usable time
    /// (the first wrap straight out of the garage) and was dropped. Either way
    /// the crossing still owes the recorder a `LapStarted`.
    fn score_pending_lap(&mut self, acc_last_time: i32, completed_laps: i32) -> Option<AdapterEvent> {
        let pending = self.pending_lap.as_mut()?;
        pending.polls += 1;
        let scored = pending.scored_at_crossing
            || acc_last_time != pending.acc_last_time
            || completed_laps > pending.completed_laps;
        if !scored && pending.polls < SCORING_GRACE_POLLS {
            return None;
        }

        let pending = self.pending_lap.take()?;
        // An unscored crossing leaves ACC's `iLastTime` describing the *previous*
        // lap — pass 0 so only the measured time is in play.
        let acc_last = if scored { acc_last_time } else { 0 };
        let lap_time_ms = resolve_lap_time_ms(acc_last, pending.measured_ms);

        let lap = (lap_time_ms >= 1_000).then(|| {
            self.lap_counter += 1;
            // Out / in-laps are recorded (tagged invalid) but don't arm the
            // return-to-garage stint split.
            self.pit_cycle.lap_completed(pending.in_pit);
            AdapterEvent::LapCompleted(LapSummary {
                lap_number: self.lap_counter,
                lap_time_ms,
                valid: pending.valid,
                sectors: sim_core::acc_cumulative_splits_to_sectors(
                    pending.s1_ms,
                    pending.s2_ms,
                    lap_time_ms,
                ),
                tyre_compound: pending.tyre_compound,
                tc_level: pending.tc_level,
                abs_level: pending.abs_level,
                fuel_used_l: None,
            })
        });

        // Every crossing starts a fresh lap. Owe the recorder a `LapStarted` so
        // it drops the truncated-lap taint and the sample buffer — ACC has no
        // lap-start signal, so a stint gap opened while idling in the garage
        // would otherwise poison the first flying lap.
        self.pending_lap_started = Some(self.lap_counter + 1);
        lap
    }



    fn reset_lap_progress(&mut self) {

        self.last_completed_laps = -1;

        self.last_physics_packet = -1;

        self.last_sector_index = -1;
        self.pending_sector_index = None;

        self.current_lap_valid = true;

        self.current_lap_in_pit = false;

        self.lap_start_compound = None;

        self.lap_start_tc = None;

        self.lap_start_abs = None;

        self.lap_start_meta_captured = false;

        // Per-lap transients only. `lap_counter` and `pit_cycle` deliberately
        // survive the AC_PAUSE blip that a return-to-garage triggers, so the
        // pit-out stint split still fires.
        self.last_norm_pos = -1.0;

        self.max_current_time_ms = 0;

        self.boundary_settled = true;

        self.pending_lap_started = None;

        self.pending_lap = None;

        self.sector_times = SectorTimes {

            s1_ms: None,

            s2_ms: None,

            s3_ms: None,

        };

    }



    /// Clear the recording-scoped lap/stint counters. Called only on a real
    /// session boundary (first announce, or a track / car / weekend-phase
    /// change) — never on the transient AC_PAUSE seen during return-to-garage.
    fn reset_stint_tracking(&mut self) {
        self.lap_counter = 0;
        self.pit_cycle.reset();
    }



    fn session_info(graphics: &GraphicsMap, statics: &AccStaticsSnapshot) -> Option<SessionInfo> {

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

            session_kind: acc_session_kind(graphics.session_type),

        })

    }



    fn session_ready(graphics: &GraphicsMap, statics: &AccStaticsSnapshot) -> bool {

        graphics.status.is_active() && Self::session_info(graphics, statics).is_some()

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



    fn detects_pit_stints(&self) -> bool {
        true
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

            self.reset_stint_tracking();

            self.last_completed_laps = graphics.completed_lap;

            self.last_sector_index = graphics.current_sector_index;
            self.pending_sector_index = None;

            if let Ok(physics) = parse_physics_map(self.physics_reader.as_ref().unwrap()) {

                self.last_physics_packet = physics.packet_id;

            }

            let info = Self::session_info(&graphics, &statics)
                .expect("session_ready guarantees session info");

            self.last_track_id = info.track_id.clone();

            self.last_car = info.car.clone();

            self.last_session_type = graphics.session_type as i32;

            self.last_session_index = graphics.session_index;

            return AdapterEvent::SessionInfo(info);

        }



        if !Self::session_ready(&graphics, &statics) {

            self.reset_lap_progress();

            return AdapterEvent::Heartbeat;

        }



        // Only treat a track_id / car / session-type change as a real session
        // boundary while the sim is actually on track. Menu navigation flips
        // statics fields without a live session and must not spawn empty
        // sessions downstream.

        if let Some(info) = Self::session_info(&graphics, &statics) {

            let track_changed = !self.last_track_id.is_empty()

                && !info.track_id.trim().is_empty()

                && !self.last_track_id.eq_ignore_ascii_case(&info.track_id);

            let car_changed = !self.last_car.is_empty()

                && !info.car.trim().is_empty()

                && !self.last_car.eq_ignore_ascii_case(info.car.trim());

            // ACC restarts `completed_lap` at 0 for each phase of a weekend, so
            // Practice → Qualifying → Race on the same track/car needs its own
            // session or the phases' lap numbers collide downstream.
            let session_changed = self.last_session_type != SESSION_UNSET
                && (graphics.session_type as i32 != self.last_session_type
                    || graphics.session_index != self.last_session_index);

            if track_changed || car_changed || session_changed {

                self.reset_lap_progress();

                self.reset_stint_tracking();

                self.last_track_id = info.track_id.clone();

                self.last_car = info.car.clone();

                self.last_session_type = graphics.session_type as i32;

                self.last_session_index = graphics.session_index;

                return AdapterEvent::SessionInfo(info);

            }

        }



        if self.last_completed_laps < 0 {

            self.last_completed_laps = graphics.completed_lap;

            self.last_sector_index = graphics.current_sector_index;
            self.pending_sector_index = None;

            if let Ok(physics) = parse_physics_map(self.physics_reader.as_ref().unwrap()) {

                self.last_physics_packet = physics.packet_id;

            }

        }



        let physics = match parse_physics_map(self.physics_reader.as_ref().unwrap()) {

            Ok(physics) => physics,

            Err(_) => {

                self.disconnect();

                return AdapterEvent::Disconnected;

            }

        };



        // Validity as of the previous tick, before this tick's reading is
        // folded in. The completion verdict below uses this so a post-line
        // `is_valid_lap == 0` (which belongs to the next lap) can't retroactively
        // invalidate the lap that just finished.
        let lap_valid_before_tick = self.current_lap_valid;

        self.update_sector_times(graphics.current_sector_index, graphics.last_sector_time);

        self.track_lap_state(&graphics, physics.speed_kmh);

        self.capture_lap_start_meta(&graphics);

        // Running peak of the live lap timer — fallback lap time for a wrap
        // that ACC never scored (out-laps).
        self.max_current_time_ms = self
            .max_current_time_ms
            .max(graphics.current_time.max(0) as u32);



        // Lap boundary: the car wrapped past the start/finish line, or ACC
        // scored a lap we didn't catch as a wrap (a poll gap). Take it once per
        // crossing — `boundary_settled` clears until the new lap is
        // unambiguously under way so the two signals can't double-fire.
        let norm = graphics.normalized_car_position;
        // `last_norm_pos` starts at -1.0, so `> 0.90` also rules out the sentinel.
        let crossed_sf = self.last_norm_pos > 0.90 && norm < 0.10;
        self.last_norm_pos = norm;

        self.resync_completed_laps(graphics.completed_lap);

        let acc_scored = self.last_completed_laps >= 0
            && graphics.completed_lap > self.last_completed_laps;

        if graphics.current_time >= BOUNDARY_SETTLE_MS {
            self.boundary_settled = true;
        }

        // A crossing already taken is waiting for ACC to put a time on it. Hold
        // telemetry until it lands: the samples from here on belong to the lap
        // that just started, and the recorder still has the finished lap's
        // buffer open.
        if self.pending_lap.is_some() {
            if acc_scored {
                self.last_completed_laps = graphics.completed_lap;
            }
            return self
                .score_pending_lap(graphics.last_time, graphics.completed_lap)
                .unwrap_or(AdapterEvent::Heartbeat);
        }

        if (crossed_sf || acc_scored) && self.boundary_settled {
            self.boundary_settled = false;
            self.last_completed_laps = graphics.completed_lap;
            self.take_lap(&graphics, lap_valid_before_tick, acc_scored);
            if acc_scored {
                // ACC scored the crossing itself — emit on the spot.
                if let Some(event) =
                    self.score_pending_lap(graphics.last_time, graphics.completed_lap)
                {
                    return event;
                }
            }
            return AdapterEvent::Heartbeat;
        } else if acc_scored {
            // Boundary already taken for this crossing; keep the tracker in step.
            self.last_completed_laps = graphics.completed_lap;
        }

        if let Some(lap_number) = self.pending_lap_started.take() {
            return AdapterEvent::LapStarted { lap_number };
        }



        // Return-to-garage / pit stop: once the car is back out of the pit lane
        // and has at least one flying lap behind it this stint, roll a new one.
        if self
            .pit_cycle
            .left_pits(graphics.is_in_pit || graphics.is_in_pit_lane)
        {
            // The out-lap now starting belongs to the new stint.
            self.current_lap_valid = true;
            return AdapterEvent::StintBoundary;
        }



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

/// Pick a lap time for a completed lap.
///
/// `acc_last` is ACC's `graphics.last_time` (`iLastTime`) — the previous
/// *scored* lap. It reads 0 or a huge sentinel until a lap has been scored this
/// run (garage out-laps), and it goes stale for the first lap or two out of a
/// pit-lane race start, holding the "session start → first crossing" duration
/// across several real crossings.
///
/// `measured` is our own peak of the live lap timer (`graphics.current_time`)
/// for the lap just run. Trust `acc_last` only when it is in a sane band *and*
/// agrees with what we measured (i.e. it really is this lap's scored time);
/// otherwise fall back to the measured time.
fn resolve_lap_time_ms(acc_last: i32, measured: u32) -> u32 {
    let acc_plausible = (1_000..=1_800_000).contains(&acc_last);
    let acc_agrees =
        measured < 1_000 || acc_last as u32 <= measured + measured / 4 + 2_000;
    if acc_plausible && acc_agrees {
        acc_last as u32
    } else if measured >= 1_000 {
        measured
    } else {
        // Neither source is usable: ACC's 0 / `i32::MAX` sentinel with no live
        // timer behind it (a race start taken from the pit lane). 0 drops the
        // lap instead of storing the sentinel as a lap time.
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_lap_time_ms, AccAdapter, PendingLap, SCORING_GRACE_POLLS};
    use sim_core::AdapterEvent;

    #[test]
    fn trusts_acc_time_on_a_normal_lap() {
        // ACC's iLastTime sits a touch above our sampled peak — trust it.
        assert_eq!(resolve_lap_time_ms(89_385, 89_200), 89_385);
        assert_eq!(resolve_lap_time_ms(92_657, 90_100), 92_657);
    }

    #[test]
    fn falls_back_when_acc_time_is_absent() {
        // Garage out-lap: iLastTime not scored yet, live timer ran a full lap.
        assert_eq!(resolve_lap_time_ms(0, 118_400), 118_400);
        assert_eq!(resolve_lap_time_ms(i32::MAX, 105_000), 105_000);
        assert_eq!(resolve_lap_time_ms(-1, 96_000), 96_000);
    }

    #[test]
    fn rejects_stale_acc_time_after_a_pit_lane_race_start() {
        // iLastTime stuck at the ~7:22 session-start span while the live timer
        // shows a clean ~92 s lap — use the measured time.
        assert_eq!(resolve_lap_time_ms(442_577, 92_640), 92_640);
    }

    #[test]
    fn keeps_the_real_pit_out_lap_time() {
        // The pit-out lap genuinely took ~7:22 wall-clock; iLastTime and the
        // live-timer peak agree, so it is kept (and marked invalid elsewhere).
        assert_eq!(resolve_lap_time_ms(442_575, 442_580), 442_575);
    }

    #[test]
    fn no_usable_time_returns_sub_second() {
        // Nothing to go on — caller drops a sub-1 s lap.
        assert!(resolve_lap_time_ms(0, 0) < 1_000);
        assert!(resolve_lap_time_ms(400, 200) < 1_000);
    }

    #[test]
    fn never_returns_an_implausible_sentinel() {
        // Race start from the pit lane: iLastTime is ACC's `i32::MAX` sentinel
        // and the live timer never ran. Storing the sentinel as a lap time is
        // worse than dropping the lap.
        assert!(resolve_lap_time_ms(i32::MAX, 0) < 1_000);
        assert!(resolve_lap_time_ms(i32::MAX, 999) < 1_000);
    }

    fn pending(measured_ms: u32, acc_last_time: i32) -> PendingLap {
        PendingLap {
            valid: true,
            in_pit: false,
            s1_ms: Some(23_000),
            s2_ms: Some(39_000),
            measured_ms,
            tyre_compound: None,
            tc_level: None,
            abs_level: None,
            acc_last_time,
            completed_laps: 4,
            scored_at_crossing: false,
            polls: 0,
        }
    }

    fn lap_time_of(event: &AdapterEvent) -> u32 {
        match event {
            AdapterEvent::LapCompleted(summary) => summary.lap_time_ms,
            other => panic!("expected a completed lap, got {other:?}"),
        }
    }

    #[test]
    fn a_held_crossing_waits_for_acc_to_score_it() {
        let mut adapter = AccAdapter::new();
        adapter.pending_lap = Some(pending(89_591, 92_627));

        // ACC has not published yet: `iLastTime` still reads the previous lap.
        assert!(adapter.score_pending_lap(92_627, 4).is_none());
        assert!(adapter.pending_lap.is_some(), "still held");

        let event = adapter
            .score_pending_lap(89_607, 5)
            .expect("scored lap is emitted");
        assert_eq!(lap_time_of(&event), 89_607, "ACC's own time wins once scored");
        assert!(adapter.pending_lap.is_none());
    }

    #[test]
    fn a_held_crossing_never_takes_the_previous_laps_time() {
        // Red Bull Ring: a 1:29.591 lap crossed the line while iLastTime still
        // held the 1:32.627 before it — near enough to sail through the
        // agreement band, which stamped every lap with the one before it.
        let mut adapter = AccAdapter::new();
        adapter.pending_lap = Some(pending(89_591, 92_627));

        let mut event = None;
        for _ in 0..SCORING_GRACE_POLLS {
            event = adapter.score_pending_lap(92_627, 4);
            if event.is_some() {
                break;
            }
        }
        let event = event.expect("the grace window falls back to the measured time");
        assert_eq!(lap_time_of(&event), 89_591, "the lap's own measured time");
    }

    #[test]
    fn a_crossing_acc_already_scored_emits_without_waiting() {
        let mut adapter = AccAdapter::new();
        let mut lap = pending(89_591, 89_607);
        lap.scored_at_crossing = true;
        adapter.pending_lap = Some(lap);

        let event = adapter
            .score_pending_lap(89_607, 4)
            .expect("no reason to wait");
        assert_eq!(lap_time_of(&event), 89_607);
    }

    #[test]
    fn a_dropped_crossing_still_owes_a_lap_started() {
        // First wrap out of the garage: no usable time, so no lap — but the next
        // lap still has to start.
        let mut adapter = AccAdapter::new();
        let mut lap = pending(0, 0);
        lap.scored_at_crossing = true;
        adapter.pending_lap = Some(lap);

        assert!(adapter.score_pending_lap(0, 4).is_none(), "lap dropped");
        assert_eq!(adapter.lap_counter, 0, "a dropped lap takes no number");
        assert_eq!(adapter.pending_lap_started, Some(1));
    }

    #[test]
    fn a_lap_counter_reset_resyncs_instead_of_stalling_scoring() {
        // Nurburgring 24h joker lap: ACC invalidates the GP-loop lap and
        // restarts `completedLaps`. Left stale at 5, the tracker would swallow
        // the next five real laps' worth of increments and none of them would
        // read as ACC-scored.
        let mut adapter = AccAdapter::new();
        adapter.last_completed_laps = 5;

        adapter.resync_completed_laps(0);
        assert_eq!(adapter.last_completed_laps, 0, "a drop is a reset");

        // The lap after the joker now scores on its own increment.
        adapter.resync_completed_laps(1);
        assert_eq!(
            adapter.last_completed_laps, 0,
            "a climb is left for the boundary to take"
        );
        assert!(1 > adapter.last_completed_laps, "and reads as ACC-scored");
    }

    #[test]
    fn an_unset_lap_counter_survives_the_resync() {
        // `-1` means "no session read yet" — `poll` owns that transition, and
        // resyncing must not claim it first.
        let mut adapter = AccAdapter::new();
        assert_eq!(adapter.last_completed_laps, -1);
        adapter.resync_completed_laps(7);
        assert_eq!(adapter.last_completed_laps, -1);
    }

    /// An adapter sitting where a lap starts: index 0, nothing captured yet.
    fn adapter_on_lap_start() -> AccAdapter {
        let mut adapter = AccAdapter::new();
        adapter.last_sector_index = 0;
        adapter
    }

    #[test]
    fn sector_lines_capture_cumulative_splits() {
        let mut adapter = adapter_on_lap_start();

        adapter.update_sector_times(1, 176_222);
        adapter.update_sector_times(2, 338_282);

        assert_eq!(adapter.sector_times.s1_ms, Some(176_222));
        assert_eq!(adapter.sector_times.s2_ms, Some(338_282));
    }

    #[test]
    fn a_split_that_lands_a_tick_late_still_files_under_its_own_sector() {
        // Nordschleife: ACC flipped to S2 while iLastSectorTime was still 0, then
        // published the split on the next tick. Before the pending slot existed
        // the index stayed at 0, so the S2 line filed cumulative-S2 as S1 and the
        // lap came back with S1 = 5:38 and S2 blank.
        let mut adapter = adapter_on_lap_start();

        adapter.update_sector_times(1, 0);
        adapter.update_sector_times(1, 176_222);
        adapter.update_sector_times(2, 338_282);

        assert_eq!(adapter.sector_times.s1_ms, Some(176_222));
        assert_eq!(adapter.sector_times.s2_ms, Some(338_282));
    }

    #[test]
    fn a_split_zeroed_across_the_line_does_not_desync_the_next_lap() {
        // The reported failure: the crossing was seen while ACC still read S3, and
        // the S3 -> S1 flip arrived with iLastSectorTime at 0. The index has to
        // advance anyway, or every line for the rest of the lap is read one sector
        // early — S1 lost, cumulative-S2 filed as the S2 split.
        let mut adapter = AccAdapter::new();
        adapter.last_sector_index = 2;

        adapter.update_sector_times(0, 0);
        assert_eq!(adapter.last_sector_index, 0, "the lap-start flip counts");

        adapter.update_sector_times(1, 176_222);
        adapter.update_sector_times(2, 338_282);

        assert_eq!(adapter.sector_times.s1_ms, Some(176_222));
        assert_eq!(adapter.sector_times.s2_ms, Some(338_282));
    }

    #[test]
    fn the_s3_line_owes_no_split() {
        // S3 is derived from the lap time at the crossing, so leaving index 2
        // must not overwrite a captured sector.
        let mut adapter = adapter_on_lap_start();
        adapter.update_sector_times(1, 176_222);
        adapter.update_sector_times(2, 338_282);

        adapter.update_sector_times(0, 517_182);

        assert_eq!(adapter.sector_times.s1_ms, Some(176_222));
        assert_eq!(adapter.sector_times.s2_ms, Some(338_282));
        assert_eq!(adapter.sector_times.s3_ms, None);
    }
}



#[cfg(not(target_os = "windows"))]

compile_error!("ACC adapter requires Windows");


