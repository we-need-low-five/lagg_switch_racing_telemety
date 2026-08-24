import type { DistanceSample, GameId } from "../types";
import {
  fuelLitersToDisplay,
  pressurePsiToDisplay,
  speedMpsToDisplay,
  tempCToDisplay,
  type UserPreferences,
} from "./preferences";

export type ChannelUnitPrefs = Pick<
  UserPreferences,
  "speedUnit" | "fuelUnit" | "tempUnit" | "pressureUnit"
>;

export interface ChartYRange {
  min: number;
  max: number;
}

/** Headroom above data max, before rounding to 2 significant digits. */
const HEADROOM = 0.08;
/** Tyre temp/pressure: pad this fraction below min and above max. */
const TYRE_PAD = 0.02;

/** Lock used when older ACC/AC laps stored `steer_angle / 450`. */
const LEGACY_STEER_LOCK_DEG = 450;

export function accSteeringIsLegacyNormalized(
  samples: DistanceSample[],
): boolean {
  let maxAbs = 0;
  for (const sample of samples) {
    const v = Number(sample.steering);
    if (!Number.isFinite(v)) continue;
    maxAbs = Math.max(maxAbs, Math.abs(v));
  }
  return maxAbs <= 1;
}

export function steeringIsDegrees(game?: GameId | null): boolean {
  return game === "acc";
}

/**
 * ACC stores wheel degrees. Older ACC laps (and AC/F1/LMU) used −1…1.
 * `|n| > 1` is already degrees and must not be scaled.
 */
export function steeringToDisplay(
  val: number,
  game?: GameId | null,
  accLegacyNormalized = false,
): number {
  const n = Number(val);
  if (!Number.isFinite(n)) return 0;
  if (game === "acc") {
    return accLegacyNormalized ? n * LEGACY_STEER_LOCK_DEG : n;
  }
  return Math.abs(n) <= 1 ? n * LEGACY_STEER_LOCK_DEG : n;
}

export function transformChannelValue(
  key: keyof DistanceSample,
  raw: number,
  prefs: ChannelUnitPrefs,
  game?: GameId | null,
  accLegacySteering = false,
): number {
  switch (key) {
    case "speed_mps":
      return speedMpsToDisplay(raw, prefs.speedUnit);
    case "throttle":
    case "brake":
      return raw * 100;
    case "steering":
      return steeringToDisplay(raw, game, accLegacySteering);
    case "fuel":
      return fuelLitersToDisplay(raw, prefs.fuelUnit);
    case "tyre_temp_fl":
    case "tyre_temp_fr":
    case "tyre_temp_rl":
    case "tyre_temp_rr":
      return tempCToDisplay(raw, prefs.tempUnit);
    case "tyre_press_fl":
    case "tyre_press_fr":
    case "tyre_press_rl":
    case "tyre_press_rr":
      return pressurePsiToDisplay(raw, prefs.pressureUnit);
    default:
      return raw;
  }
}

export function collectDisplayValues(
  sampleLists: DistanceSample[][],
  channelKey: keyof DistanceSample,
  prefs: ChannelUnitPrefs,
  game?: GameId | null,
): number[] {
  const values: number[] = [];
  for (const samples of sampleLists) {
    const accLegacySteering =
      channelKey === "steering" &&
      game === "acc" &&
      accSteeringIsLegacyNormalized(samples);
    for (const sample of samples) {
      const raw = Number(sample[channelKey]);
      if (!Number.isFinite(raw)) continue;
      values.push(
        transformChannelValue(
          channelKey,
          raw,
          prefs,
          game,
          accLegacySteering,
        ),
      );
    }
  }
  return values;
}

function isFixedZeroToHundred(key: keyof DistanceSample): boolean {
  return key === "throttle" || key === "brake";
}

function isSymmetric(key: keyof DistanceSample): boolean {
  return key === "steering" || key === "lap_time_s";
}

function isTyreChannel(key: keyof DistanceSample): boolean {
  return (
    key === "tyre_temp_fl" ||
    key === "tyre_temp_fr" ||
    key === "tyre_temp_rl" ||
    key === "tyre_temp_rr" ||
    key === "tyre_press_fl" ||
    key === "tyre_press_fr" ||
    key === "tyre_press_rl" ||
    key === "tyre_press_rr"
  );
}

function twoSigStep(value: number): number {
  const abs = Math.abs(value);
  if (!Number.isFinite(abs) || abs === 0) return 1;
  const exp = Math.floor(Math.log10(abs));
  return 10 ** (exp - 1);
}

function snapCeil2Sig(value: number): number {
  if (!Number.isFinite(value)) return 1;
  if (value === 0) return 0;
  const step = twoSigStep(value);
  return Math.ceil(value / step - 1e-12) * step;
}

function snapFloor2Sig(value: number): number {
  if (!Number.isFinite(value)) return 0;
  if (value === 0) return 0;
  const step = twoSigStep(value);
  return Math.floor(value / step + 1e-12) * step;
}

/**
 * Add ~8% headroom, then round up to 2 significant digits.
 * Coarse 1–2–5 snapping is avoided: 100 × 1.08 must not become 200.
 */
export function niceCeil(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 1;
  return snapCeil2Sig(value * (1 + HEADROOM));
}

function tyreYRange(displayValues: number[]): ChartYRange {
  if (displayValues.length === 0) {
    return { min: 0, max: 1 };
  }
  const dataMin = Math.min(...displayValues);
  const dataMax = Math.max(...displayValues);
  let min = snapFloor2Sig(dataMin * (1 - TYRE_PAD));
  let max = snapCeil2Sig(dataMax * (1 + TYRE_PAD));
  if (!(max > min)) {
    const mid = (dataMin + dataMax) / 2;
    const bump = Math.max(Math.abs(mid) * TYRE_PAD, twoSigStep(mid || 1));
    min = snapFloor2Sig(mid - bump);
    max = snapCeil2Sig(mid + bump);
  }
  if (!(max > min)) {
    return { min: dataMin - 1, max: dataMax + 1 };
  }
  return { min, max };
}

export function yRangeForChannel(
  channelKey: keyof DistanceSample,
  displayValues: number[],
): ChartYRange {
  if (isFixedZeroToHundred(channelKey)) {
    return { min: 0, max: 100 };
  }

  const finite = displayValues.filter((v) => Number.isFinite(v));

  if (isSymmetric(channelKey)) {
    if (finite.length === 0) {
      const extent = niceCeil(0);
      return { min: -extent, max: extent };
    }
    const dataMin = Math.min(...finite);
    const dataMax = Math.max(...finite);
    // Time delta is rebased at mini-sector entry, so the visible span can
    // be max-min of the full lap, which may exceed peak |value|.
    const peak = Math.max(
      Math.abs(dataMin),
      Math.abs(dataMax),
      channelKey === "lap_time_s" ? dataMax - dataMin : 0,
    );
    const extent = niceCeil(peak);
    return { min: -extent, max: extent };
  }

  if (isTyreChannel(channelKey)) {
    return tyreYRange(finite);
  }

  const dataMax = finite.length ? Math.max(...finite) : 0;
  return { min: 0, max: niceCeil(dataMax) };
}
