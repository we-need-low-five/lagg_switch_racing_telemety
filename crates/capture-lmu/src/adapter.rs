use crate::lmu_maps::{
    c_str, scoring_info_offset, telem_info_offset, telemetry_header_offset, veh_scoring_offset,
    ScoringInfoV01, TelemInfoV01, VehicleScoringInfoV01, LMU_DATA_NAME, MAX_MAPPED_VEHICLES,
};
use chrono::Utc;
use sim_capture_common::SharedMemoryMapping;
use sim_core::{
    acc_cumulative_splits_to_sectors, normalize_brake, normalize_steering, normalize_throttle,
    AdapterEvent, GameAdapter, GameId, LapSummary, SessionInfo, TelemetrySample,
};

/// Polls the session clock may stay frozen before we stop treating LMU as live.
/// When the game exits, Windows keeps our `LMU_Data` handle — and therefore the
/// last frame — readable indefinitely, so a session clock (`mElapsedTime`) that
/// has stopped advancing is what tells us the sim is gone or paused. ~90 ms at
/// the 3 ms recorder poll; the recorder's abandon timeout finalizes from there.
const STALE_FRAME_LIMIT: u32 = 30;

const G: f64 = 9.806_65;

pub struct LmuAdapter {
    mapping: Option<SharedMemoryMapping>,
    session_announced: bool,
    last_track_id: String,
    last_car: String,
    /// Current lap number last seen on the player's telemetry (`mLapNumber`).
    /// -1 until the player starts the first flying/out lap.
    last_lap: i32,
    /// OR of `mInPits` across the current lap — invalidates the lap on completion.
    pit_this_lap: bool,
    last_elapsed_time: f64,
    stale_frame_polls: u32,
}

impl LmuAdapter {
    pub fn new() -> Self {
        Self {
            mapping: None,
            session_announced: false,
            last_track_id: String::new(),
            last_car: String::new(),
            last_lap: -1,
            pit_this_lap: false,
            last_elapsed_time: f64::NAN,
            stale_frame_polls: 0,
        }
    }

    fn connect(&mut self) -> bool {
        if self.mapping.is_some() {
            return true;
        }
        // size 0 => map the whole section; sub-structs are read at computed offsets.
        self.mapping = SharedMemoryMapping::open(LMU_DATA_NAME, 0).ok();
        self.mapping.is_some()
    }

    /// Locate the player's row in `vehScoringInfo` (order need not match
    /// `telemInfo`), matching on slot id and falling back to the `mIsPlayer` flag.
    fn player_scoring(
        map: &SharedMemoryMapping,
        num_vehicles: i32,
        player_id: i32,
    ) -> Option<VehicleScoringInfoV01> {
        let count = (num_vehicles.max(0) as usize).min(MAX_MAPPED_VEHICLES);
        let is_player_off = core::mem::offset_of!(VehicleScoringInfoV01, mIsPlayer);
        let mut fallback = None;
        for i in 0..count {
            let base = veh_scoring_offset(i);
            let id: i32 = map.read_pod_at(base);
            if id == player_id {
                return Some(map.read_pod_at::<VehicleScoringInfoV01>(base));
            }
            if fallback.is_none() {
                let is_player: u8 = map.read_pod_at(base + is_player_off);
                if is_player != 0 {
                    fallback = Some(map.read_pod_at::<VehicleScoringInfoV01>(base));
                }
            }
        }
        fallback
    }

    fn session_info(tel: &TelemInfoV01, track_id: String) -> SessionInfo {
        SessionInfo {
            game: GameId::Lmu,
            track_id,
            track: c_str(&tel.mTrackName),
            car: c_str(&tel.mVehicleName),
            game_version: "LMU".to_string(),
            player_name: String::new(),
        }
    }
}

impl Default for LmuAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GameAdapter for LmuAdapter {
    fn game_id(&self) -> GameId {
        GameId::Lmu
    }

    fn is_active(&self) -> bool {
        self.mapping.is_some()
    }

    fn poll(&mut self) -> AdapterEvent {
        if !self.connect() {
            return AdapterEvent::Disconnected;
        }
        let map = self.mapping.as_ref().unwrap();

        // telemetry header: activeVehicles, playerVehicleIdx, playerHasVehicle
        let hdr = telemetry_header_offset();
        let active_vehicles: u8 = map.read_pod_at(hdr);
        let player_idx: u8 = map.read_pod_at(hdr + 1);
        let player_has_vehicle: u8 = map.read_pod_at(hdr + 2);

        let no_car = active_vehicles == 0
            || player_has_vehicle == 0
            || player_idx as usize >= MAX_MAPPED_VEHICLES;
        if no_car {
            return if self.session_announced {
                AdapterEvent::Heartbeat
            } else {
                AdapterEvent::Disconnected
            };
        }

        let tel: TelemInfoV01 = map.read_pod_at(telem_info_offset(player_idx as usize));

        // Frozen session clock => sim paused or gone. Keep recording only if the
        // clock is stuck while the car is clearly still moving.
        let elapsed = tel.mElapsedTime;
        let moving = tel.mLocalVel.magnitude() > 0.5;
        if self.session_announced && elapsed == self.last_elapsed_time {
            self.stale_frame_polls = self.stale_frame_polls.saturating_add(1);
            if !(self.stale_frame_polls >= STALE_FRAME_LIMIT && moving) {
                return AdapterEvent::Heartbeat;
            }
        } else {
            self.stale_frame_polls = 0;
        }
        self.last_elapsed_time = elapsed;

        let scoring_info_off = scoring_info_offset();
        let num_vehicles: i32 = map.read_pod_at(
            scoring_info_off + core::mem::offset_of!(ScoringInfoV01, mNumVehicles),
        );
        let in_realtime: u8 = map
            .read_pod_at(scoring_info_off + core::mem::offset_of!(ScoringInfoV01, mInRealtime));
        let scoring = Self::player_scoring(map, num_vehicles, tel.mID);

        let track = c_str(&tel.mTrackName);
        let car = c_str(&tel.mVehicleName);
        let track_id = slugify_track_id(&track);

        // Announce once the player is actually in the car on track (not the
        // monitor / menus) — avoids empty sessions and bogus splits on reload.
        if !self.session_announced {
            if in_realtime == 0 || track.is_empty() || car.is_empty() {
                return AdapterEvent::Heartbeat;
            }
            self.session_announced = true;
            self.last_track_id = track_id.clone();
            self.last_car = car.clone();
            self.last_lap = -1;
            self.pit_this_lap = false;
            return AdapterEvent::SessionInfo(Self::session_info(&tel, track_id));
        }

        let track_changed = !track_id.is_empty()
            && !self.last_track_id.is_empty()
            && !self.last_track_id.eq_ignore_ascii_case(&track_id);
        let car_changed = !car.is_empty()
            && !self.last_car.is_empty()
            && !self.last_car.eq_ignore_ascii_case(&car);
        if track_changed || car_changed {
            self.last_track_id = track_id.clone();
            self.last_car = car.clone();
            self.last_lap = -1;
            self.pit_this_lap = false;
            return AdapterEvent::SessionInfo(Self::session_info(&tel, track_id));
        }

        let in_pits = scoring.as_ref().map(|s| s.mInPits != 0).unwrap_or(false);
        if in_pits {
            self.pit_this_lap = true;
        }

        let lap_number = tel.mLapNumber;

        // Lap completed: mLapNumber has ticked past the lap we were on. The
        // finished lap's time and cumulative splits are in the player's scoring,
        // which lags telemetry by a frame or two — hold the boundary until it
        // catches up rather than advancing `last_lap` and losing the lap.
        if self.last_lap >= 1 && lap_number > self.last_lap {
            if let Some(s) = scoring {
                let completed = self.last_lap as u32;
                self.last_lap = lap_number;
                let pitted = std::mem::take(&mut self.pit_this_lap);
                let lap_time_ms = (s.mLastLapTime * 1000.0).max(0.0) as u32;
                let cum_s1 = (s.mLastSector1 > 0.0).then_some((s.mLastSector1 * 1000.0) as u32);
                let cum_s2 = (s.mLastSector2 > 0.0).then_some((s.mLastSector2 * 1000.0) as u32);
                let summary = LapSummary {
                    lap_number: completed,
                    lap_time_ms,
                    valid: lap_time_ms > 0 && tel.mLapInvalidated == 0 && !pitted,
                    sectors: acc_cumulative_splits_to_sectors(cum_s1, cum_s2, lap_time_ms),
                    tyre_compound: Some(c_str(&tel.mFrontTireCompoundName)).filter(|s| !s.is_empty()),
                    tc_level: Some(tel.mTC as i32),
                    abs_level: Some(tel.mABS as i32),
                    fuel_used_l: None,
                };
                return AdapterEvent::LapCompleted(summary);
            }
            return AdapterEvent::Heartbeat;
        }

        // First lap boundary once the player leaves the garage.
        if self.last_lap < 0 {
            if lap_number < 1 {
                return AdapterEvent::Heartbeat;
            }
            self.last_lap = lap_number;
            self.pit_this_lap = in_pits;
            return AdapterEvent::LapStarted {
                lap_number: lap_number.max(1) as u32,
            };
        }

        self.last_lap = lap_number;

        let wheels = tel.mWheel;
        let lap_dist = scoring.as_ref().map(|s| s.mLapDist).unwrap_or(0.0);
        let lap_time_s = (tel.mElapsedTime - tel.mLapStartET).max(0.0) as f32;

        // Copy packed fields into locals before the `json!` macro (it borrows).
        let (pos, local_vel, local_accel) = (tel.mPos, tel.mLocalVel, tel.mLocalAccel);
        let lap_invalidated = tel.mLapInvalidated;
        let current_sector = tel.mCurrentSector;
        let clutch = tel.mUnfilteredClutch;
        let steering_shaft_torque = tel.mSteeringShaftTorque;
        let rear_brake_bias = tel.mRearBrakeBias;
        let turbo_boost = tel.mTurboBoostPressure;
        let engine_water_temp = tel.mEngineWaterTemp;
        let engine_oil_temp = tel.mEngineOilTemp;
        let abs_active = tel.mABSActive;
        let tc_active = tel.mTCActive;
        let tc = tel.mTC;
        let abs = tel.mABS;
        let battery_charge = tel.mBatteryChargeFraction;
        let state_of_charge = tel.mSoC;
        let virtual_energy = tel.mVirtualEnergy;
        let regen_kw = tel.mRegen;
        let front_ride_height = tel.mFrontRideHeight;
        let rear_ride_height = tel.mRearRideHeight;
        let brake_temp_c = [
            wheels[0].brake_temp_c(), wheels[1].brake_temp_c(),
            wheels[2].brake_temp_c(), wheels[3].brake_temp_c(),
        ];
        let tyre_wear = [
            wheels[0].wear_fraction(), wheels[1].wear_fraction(),
            wheels[2].wear_fraction(), wheels[3].wear_fraction(),
        ];
        let tyre_carcass_temp_c = [
            wheels[0].carcass_temp_c(), wheels[1].carcass_temp_c(),
            wheels[2].carcass_temp_c(), wheels[3].carcass_temp_c(),
        ];

        AdapterEvent::Telemetry(TelemetrySample {
            timestamp: Utc::now(),
            lap_time_s,
            distance_m: lap_dist as f32,
            speed_mps: local_vel.magnitude() as f32,
            throttle: normalize_throttle(tel.mUnfilteredThrottle as f32),
            brake: normalize_brake(tel.mUnfilteredBrake as f32),
            steering: normalize_steering(tel.mUnfilteredSteering as f32),
            gear: tel.mGear,
            rpm: tel.mEngineRPM as f32,
            pos_x: pos.x as f32,
            pos_y: pos.y as f32,
            pos_z: pos.z as f32,
            fuel: Some(tel.mFuel as f32),
            tyre_temp_fl: Some(wheels[0].temp_centre_c() as f32),
            tyre_temp_fr: Some(wheels[1].temp_centre_c() as f32),
            tyre_temp_rl: Some(wheels[2].temp_centre_c() as f32),
            tyre_temp_rr: Some(wheels[3].temp_centre_c() as f32),
            tyre_press_fl: Some(wheels[0].pressure_kpa() as f32),
            tyre_press_fr: Some(wheels[1].pressure_kpa() as f32),
            tyre_press_rl: Some(wheels[2].pressure_kpa() as f32),
            tyre_press_rr: Some(wheels[3].pressure_kpa() as f32),
            // rF2 local accel: x = lateral, y = vertical, z = longitudinal (matches schema).
            g_force_x: Some((local_accel.x / G) as f32),
            g_force_y: Some((local_accel.y / G) as f32),
            g_force_z: Some((local_accel.z / G) as f32),
            slip_angle_fl: Some(wheels[0].slip_angle_deg() as f32),
            slip_angle_fr: Some(wheels[1].slip_angle_deg() as f32),
            slip_angle_rl: Some(wheels[2].slip_angle_deg() as f32),
            slip_angle_rr: Some(wheels[3].slip_angle_deg() as f32),
            raw: serde_json::json!({
                "in_pits": in_pits as u8,
                "lap_invalidated": lap_invalidated,
                "current_sector": current_sector,
                "clutch": clutch,
                "steering_shaft_torque_nm": steering_shaft_torque,
                "rear_brake_bias": rear_brake_bias,
                "turbo_boost_pa": turbo_boost,
                "engine_water_temp_c": engine_water_temp,
                "engine_oil_temp_c": engine_oil_temp,
                "abs_active": abs_active,
                "tc_active": tc_active,
                "tc": tc,
                "abs": abs,
                // hybrid / energy — LMH/LMDh and the WEC virtual-energy rules
                "battery_charge_fraction": battery_charge,
                "state_of_charge_pct": state_of_charge,
                "virtual_energy_pct": virtual_energy,
                "regen_kw": regen_kw,
                "front_ride_height_m": front_ride_height,
                "rear_ride_height_m": rear_ride_height,
                "brake_temp_c": brake_temp_c,
                "tyre_wear": tyre_wear,
                "tyre_carcass_temp_c": tyre_carcass_temp_c,
            }),
        })
    }
}

fn slugify_track_id(track: &str) -> String {
    track
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "_")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmu_maps::{SharedMemoryObjectOut, TelemVect3};

    #[test]
    fn layout_is_pinned() {
        // The `const _` assertions in lmu_maps already gate compilation; this
        // keeps the intent visible in the test surface.
        assert_eq!(core::mem::size_of::<SharedMemoryObjectOut>(), 324_820);
        assert_eq!(core::mem::size_of::<TelemInfoV01>(), 1_888);
        assert_eq!(core::mem::size_of::<VehicleScoringInfoV01>(), 584);
    }

    #[test]
    fn slugify_track_id_normalizes() {
        assert_eq!(slugify_track_id("  Circuit de Spa-Francorchamps "), "circuit_de_spafrancorchamps");
        assert_eq!(slugify_track_id("Sebring"), "sebring");
    }

    #[test]
    fn vect3_magnitude() {
        let v = TelemVect3 { x: 3.0, y: 0.0, z: 4.0 };
        assert!((v.magnitude() - 5.0).abs() < 1e-9);
    }
}
