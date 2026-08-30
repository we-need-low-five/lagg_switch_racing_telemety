//! Rust mirror of Le Mans Ultimate's official shared-memory layout.
//!
//! The mapping named `LMU_Data` contains a single [`SharedMemoryLayout`], which
//! is the S397 rFactor 2 plugin layout (`Support/SharedMemoryInterface/*.hpp`
//! in the game install) wrapped in a few container structs. The headers are
//! compiled `#pragma pack(4)`, so every struct here is `#[repr(C, packed(4))]`
//! and the compile-time size assertions below pin the layout — if the game ever
//! changes a struct, the crate stops compiling instead of silently reading
//! garbage offsets (which is exactly the bug this file replaces).
//!
//! Windows type mapping: `long`/`unsigned long` are 32-bit (LLP64), pointers and
//! `size_t`/`unsigned long long` are 64-bit, `bool`/`char`/enum-of-uint8 are one
//! byte. C++ `bool` bytes are read as `u8` (an arbitrary byte is not a valid
//! Rust `bool`).

#![allow(non_snake_case)]

use core::mem::{offset_of, size_of};

pub const LMU_DATA_NAME: &str = "Local\\LMU_Data";

/// `SharedMemoryEvent` enum count (`SME_MAX`).
const SME_MAX: usize = 17;
/// `vehScoringInfo` / `telemInfo` fixed array length.
pub const MAX_MAPPED_VEHICLES: usize = 104;

#[repr(C, packed(4))]
#[derive(Clone, Copy, Default)]
pub struct TelemVect3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl TelemVect3 {
    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct TelemWheelV01 {
    pub mSuspensionDeflection: f64,
    pub mRideHeight: f64,
    pub mSuspForce: f64,
    pub mBrakeTemp: f64,
    pub mBrakePressure: f64,
    pub mRotation: f64,
    pub mLateralPatchVel: f64,
    pub mLongitudinalPatchVel: f64,
    pub mLateralGroundVel: f64,
    pub mLongitudinalGroundVel: f64,
    pub mCamber: f64,
    pub mLateralForce: f64,
    pub mLongitudinalForce: f64,
    pub mTireLoad: f64,
    pub mGripFract: f64,
    pub mPressure: f64,
    pub mTemperature: [f64; 3],
    pub mWear: f64,
    pub mTerrainName: [u8; 16],
    pub mSurfaceType: u8,
    pub mFlat: u8,
    pub mDetached: u8,
    pub mStaticUndeflectedRadius: u8,
    pub mVerticalTireDeflection: f64,
    pub mWheelYLocation: f64,
    pub mToe: f64,
    pub mTireCarcassTemperature: f64,
    pub mTireInnerLayerTemperature: [f64; 3],
    pub mOptimalTemp: f32,
    pub mCompoundIndex: u8,
    pub mCompoundType: u8,
    pub mExpansion: [u8; 18],
}

impl TelemWheelV01 {
    /// Centre tread temperature in Celsius (`mTemperature` is Kelvin, L/C/R).
    pub fn temp_centre_c(&self) -> f64 {
        let t = self.mTemperature;
        t[1] - 273.15
    }
    /// Tyre pressure in kPa.
    pub fn pressure_kpa(&self) -> f64 {
        self.mPressure
    }
    pub fn brake_temp_c(&self) -> f64 {
        self.mBrakeTemp
    }
    /// Wear as a fraction of maximum (0.0 = new, 1.0 = worn out).
    pub fn wear_fraction(&self) -> f64 {
        self.mWear
    }
    pub fn carcass_temp_c(&self) -> f64 {
        self.mTireCarcassTemperature - 273.15
    }
    /// Contact-patch slip angle in degrees, from ground-relative patch velocity.
    pub fn slip_angle_deg(&self) -> f64 {
        let lat = self.mLateralGroundVel;
        let lon = self.mLongitudinalGroundVel.abs();
        if lon < 0.5 && lat.abs() < 0.5 {
            0.0
        } else {
            lat.atan2(lon).to_degrees()
        }
    }
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct TelemInfoV01 {
    pub mID: i32,
    pub mDeltaTime: f64,
    pub mElapsedTime: f64,
    pub mLapNumber: i32,
    pub mLapStartET: f64,
    pub mVehicleName: [u8; 64],
    pub mTrackName: [u8; 64],

    pub mPos: TelemVect3,
    pub mLocalVel: TelemVect3,
    pub mLocalAccel: TelemVect3,

    pub mOri: [TelemVect3; 3],
    pub mLocalRot: TelemVect3,
    pub mLocalRotAccel: TelemVect3,

    pub mGear: i32,
    pub mEngineRPM: f64,
    pub mEngineWaterTemp: f64,
    pub mEngineOilTemp: f64,
    pub mClutchRPM: f64,

    pub mUnfilteredThrottle: f64,
    pub mUnfilteredBrake: f64,
    pub mUnfilteredSteering: f64,
    pub mUnfilteredClutch: f64,

    pub mFilteredThrottle: f64,
    pub mFilteredBrake: f64,
    pub mFilteredSteering: f64,
    pub mFilteredClutch: f64,

    pub mSteeringShaftTorque: f64,
    pub mFront3rdDeflection: f64,
    pub mRear3rdDeflection: f64,

    pub mFrontWingHeight: f64,
    pub mFrontRideHeight: f64,
    pub mRearRideHeight: f64,
    pub mDrag: f64,
    pub mFrontDownforce: f64,
    pub mRearDownforce: f64,

    pub mFuel: f64,
    pub mEngineMaxRPM: f64,
    pub mScheduledStops: u8,
    pub mOverheating: u8,
    pub mDetached: u8,
    pub mHeadlights: u8,
    pub mDentSeverity: [u8; 8],
    pub mLastImpactET: f64,
    pub mLastImpactMagnitude: f64,
    pub mLastImpactPos: TelemVect3,

    pub mEngineTorque: f64,
    pub mCurrentSector: i32,
    pub mSpeedLimiter: u8,
    pub mMaxGears: u8,
    pub mFrontTireCompoundIndex: u8,
    pub mRearTireCompoundIndex: u8,
    pub mFuelCapacity: f64,
    pub mFrontFlapActivated: u8,
    pub mRearFlapActivated: u8,
    pub mRearFlapLegalStatus: u8,
    pub mIgnitionStarter: u8,

    pub mFrontTireCompoundName: [u8; 18],
    pub mRearTireCompoundName: [u8; 18],

    pub mSpeedLimiterAvailable: u8,
    pub mAntiStallActivated: u8,
    pub mUnused: [u8; 2],
    pub mVisualSteeringWheelRange: f32,

    pub mRearBrakeBias: f64,
    pub mTurboBoostPressure: f64,
    pub mPhysicsToGraphicsOffset: [f32; 3],
    pub mPhysicalSteeringWheelRange: f32,

    pub mDeltaBest: f64,
    pub mBatteryChargeFraction: f64,

    pub mElectricBoostMotorTorque: f64,
    pub mElectricBoostMotorRPM: f64,
    pub mElectricBoostMotorTemperature: f64,
    pub mElectricBoostWaterTemperature: f64,
    pub mElectricBoostMotorState: u8,
    pub mLapInvalidated: u8,
    pub mABSActive: u8,
    pub mTCActive: u8,
    pub mSpeedLimiterActive: u8,
    pub mWiperState: u8,
    pub mTC: u8,
    pub mTCMax: u8,
    pub mTCSlip: u8,
    pub mTCSlipMax: u8,
    pub mTCCut: u8,
    pub mTCCutMax: u8,
    pub mABS: u8,
    pub mABSMax: u8,
    pub mMotorMap: u8,
    pub mMotorMapMax: u8,
    pub mMigration: u8,
    pub mMigrationMax: u8,
    pub mFrontAntiSway: u8,
    pub mFrontAntiSwayMax: u8,
    pub mRearAntiSway: u8,
    pub mRearAntiSwayMax: u8,
    pub mLiftAndCoastProgress: u8,
    pub mTrackLimitsSteps: u8,
    pub mRegen: f32,
    pub mSoC: f32,
    pub mVirtualEnergy: f32,
    pub mTimeGapCarAhead: f32,
    pub mTimeGapCarBehind: f32,
    pub mTimeGapPlaceAhead: f32,
    pub mTimeGapPlaceBehind: f32,
    pub mVehicleModel: [u8; 30],
    pub mVehicleClass: u8,
    pub mVehicleChampionship: u8,

    pub mExpansion: [u8; 20],

    pub mWheel: [TelemWheelV01; 4],
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct VehicleScoringInfoV01 {
    pub mID: i32,
    pub mDriverName: [u8; 32],
    pub mVehicleName: [u8; 64],
    pub mTotalLaps: i16,
    pub mSector: i8,
    pub mFinishStatus: i8,
    pub mLapDist: f64,
    pub mPathLateral: f64,
    pub mTrackEdge: f64,

    pub mBestSector1: f64,
    pub mBestSector2: f64,
    pub mBestLapTime: f64,
    pub mLastSector1: f64,
    pub mLastSector2: f64,
    pub mLastLapTime: f64,
    pub mCurSector1: f64,
    pub mCurSector2: f64,

    pub mNumPitstops: i16,
    pub mNumPenalties: i16,
    pub mIsPlayer: u8,

    pub mControl: i8,
    pub mInPits: u8,
    pub mPlace: u8,
    pub mVehicleClass: [u8; 32],

    pub mTimeBehindNext: f64,
    pub mLapsBehindNext: i32,
    pub mTimeBehindLeader: f64,
    pub mLapsBehindLeader: i32,
    pub mLapStartET: f64,

    pub mPos: TelemVect3,
    pub mLocalVel: TelemVect3,
    pub mLocalAccel: TelemVect3,

    pub mOri: [TelemVect3; 3],
    pub mLocalRot: TelemVect3,
    pub mLocalRotAccel: TelemVect3,

    pub mHeadlights: u8,
    pub mPitState: u8,
    pub mServerScored: u8,
    pub mIndividualPhase: u8,

    pub mQualification: i32,

    pub mTimeIntoLap: f64,
    pub mEstimatedLapTime: f64,

    pub mPitGroup: [u8; 24],
    pub mFlag: u8,
    pub mUnderYellow: u8,
    pub mCountLapFlag: u8,
    pub mInGarageStall: u8,

    pub mUpgradePack: [u8; 16],
    pub mPitLapDist: f32,

    pub mBestLapSector1: f32,
    pub mBestLapSector2: f32,

    pub mSteamID: u64,

    pub mVehFilename: [u8; 32],

    pub mAttackMode: i16,
    pub mFuelFraction: u8,
    pub mDRSState: u8,

    pub mExpansion: [u8; 4],
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct ScoringInfoV01 {
    pub mTrackName: [u8; 64],
    pub mSession: i32,
    pub mCurrentET: f64,
    pub mEndET: f64,
    pub mMaxLaps: i32,
    pub mLapDist: f64,
    pub mResultsStream: u64, // char*
    pub mNumVehicles: i32,
    pub mGamePhase: u8,
    pub mYellowFlagState: i8,
    pub mSectorFlag: [i8; 3],
    pub mStartLight: u8,
    pub mNumRedLights: u8,
    pub mInRealtime: u8,
    pub mPlayerName: [u8; 32],
    pub mPlrFileName: [u8; 64],

    pub mDarkCloud: f64,
    pub mRaining: f64,
    pub mAmbientTemp: f64,
    pub mTrackTemp: f64,
    pub mWind: TelemVect3,
    pub mMinPathWetness: f64,
    pub mMaxPathWetness: f64,

    pub mGameMode: u8,
    pub mIsPasswordProtected: u8,
    pub mServerPort: u16,
    pub mServerPublicIP: u32,
    pub mMaxPlayers: i32,
    pub mServerName: [u8; 32],
    pub mStartET: f32,

    pub mAvgPathWetness: f64,
    pub mSessionTimeRemaining: f32,
    pub mTimeOfDay: f32,
    pub mIsFixedSetup: u8,
    pub mTrackGripLevel: u8,
    pub mCloudCoverage: u8,
    pub mTrackLimitsStepsPerPenalty: u8,
    pub mTrackLimitsStepsPerPoint: u8,
    pub mExpansion: [u8; 187],

    pub mVehicle: u64, // VehicleScoringInfoV01*
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct ApplicationStateV01 {
    pub mAppWindow: u64, // HWND
    pub mWidth: u32,
    pub mHeight: u32,
    pub mRefreshRate: u32,
    pub mWindowed: u32,
    pub mOptionsLocation: u8,
    pub mOptionsPage: [u8; 31],
    pub mExpansion: [u8; 204],
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct SharedMemoryGeneric {
    pub events: [u32; SME_MAX],
    pub gameVersion: i32,
    pub FFBTorque: f32,
    pub appInfo: ApplicationStateV01,
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct SharedMemoryPathData {
    /// UserData, CustomVariables, StewardResults, PlayerProfile, PluginsFolder.
    pub paths: [[u8; 260]; 5],
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct SharedMemoryScoringData {
    pub scoringInfo: ScoringInfoV01,
    pub scoringStreamSize: u64, // size_t
    pub vehScoringInfo: [VehicleScoringInfoV01; MAX_MAPPED_VEHICLES],
    pub scoringStream: [u8; 65536],
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct SharedMemoryTelemetryData {
    pub activeVehicles: u8,
    pub playerVehicleIdx: u8,
    pub playerHasVehicle: u8,
    pub telemInfo: [TelemInfoV01; MAX_MAPPED_VEHICLES],
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct SharedMemoryObjectOut {
    pub generic: SharedMemoryGeneric,
    pub paths: SharedMemoryPathData,
    pub scoring: SharedMemoryScoringData,
    pub telemetry: SharedMemoryTelemetryData,
}

// --- Layout pins ---------------------------------------------------------------
// Sizes computed by hand from the packed(4) headers; a mismatch here means the
// game's layout moved and the offsets below would be wrong.
const _: () = {
    assert!(size_of::<TelemVect3>() == 24);
    assert!(size_of::<TelemWheelV01>() == 260);
    assert!(size_of::<TelemInfoV01>() == 1888);
    assert!(size_of::<VehicleScoringInfoV01>() == 584);
    assert!(size_of::<ScoringInfoV01>() == 548);
    assert!(size_of::<ApplicationStateV01>() == 260);
    assert!(size_of::<SharedMemoryGeneric>() == 336);
    assert!(size_of::<SharedMemoryPathData>() == 1300);
    assert!(size_of::<SharedMemoryScoringData>() == 126828);
    assert!(size_of::<SharedMemoryTelemetryData>() == 196356);
    assert!(size_of::<SharedMemoryObjectOut>() == 324820);

    // Field offsets the poll loop reads directly.
    assert!(offset_of!(TelemInfoV01, mElapsedTime) == 12);
    assert!(offset_of!(TelemInfoV01, mLapNumber) == 20);
    assert!(offset_of!(TelemInfoV01, mWheel) == 848);
    assert!(offset_of!(VehicleScoringInfoV01, mIsPlayer) == 196);
    assert!(offset_of!(SharedMemoryTelemetryData, telemInfo) == 4);
};

/// Byte offset of `telemetry.telemInfo[idx]` within the mapping.
pub fn telem_info_offset(idx: usize) -> usize {
    offset_of!(SharedMemoryObjectOut, telemetry)
        + offset_of!(SharedMemoryTelemetryData, telemInfo)
        + idx * size_of::<TelemInfoV01>()
}

/// Byte offset of `scoring.vehScoringInfo[idx]` within the mapping.
pub fn veh_scoring_offset(idx: usize) -> usize {
    offset_of!(SharedMemoryObjectOut, scoring)
        + offset_of!(SharedMemoryScoringData, vehScoringInfo)
        + idx * size_of::<VehicleScoringInfoV01>()
}

/// Byte offset of `scoring.scoringInfo` within the mapping.
pub fn scoring_info_offset() -> usize {
    offset_of!(SharedMemoryObjectOut, scoring) + offset_of!(SharedMemoryScoringData, scoringInfo)
}

/// Byte offset of the `telemetry` header (`activeVehicles`, `playerVehicleIdx`,
/// `playerHasVehicle` are the first three bytes).
pub fn telemetry_header_offset() -> usize {
    offset_of!(SharedMemoryObjectOut, telemetry)
}

/// Trim a fixed C `char` buffer at the first NUL and decode as UTF-8 (lossy).
pub fn c_str(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).trim().to_string()
}
