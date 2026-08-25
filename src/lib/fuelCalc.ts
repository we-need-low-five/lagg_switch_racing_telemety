export interface FuelCalcInput {
  hours: number;
  minutes: number;
  lapMinutes: number;
  lapSeconds: number;
  lapMilliseconds: number;
  fuelPerLapL: number;
  safetyMargin: boolean;
}

export interface FuelCalcResult {
  laps: number;
  baseFuelL: number;
  marginFuelL: number;
  totalFuelL: number;
}

export function lapTimeFromParts(
  minutes: number,
  seconds: number,
  milliseconds: number,
): number | null {
  const m = Number.isFinite(minutes) ? Math.max(0, minutes) : 0;
  const s = Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
  const ms = Number.isFinite(milliseconds) ? Math.max(0, milliseconds) : 0;

  if (m === 0 && s === 0 && ms === 0) return null;

  if (s >= 60 || ms >= 1000) return null;

  return m * 60 + s + ms / 1000;
}

export function lapTimePartsFromMs(ms: number): {
  minutes: number;
  seconds: number;
  milliseconds: number;
} {
  const clamped = Math.max(0, Math.round(ms));
  return {
    minutes: Math.floor(clamped / 60_000),
    seconds: Math.floor((clamped % 60_000) / 1000),
    milliseconds: clamped % 1000,
  };
}

export function raceDurationSeconds(hours: number, minutes: number): number {
  const h = Number.isFinite(hours) ? Math.max(0, hours) : 0;
  const m = Number.isFinite(minutes) ? Math.max(0, minutes) : 0;
  return h * 3600 + m * 60;
}

const SAFETY_MARGIN_LAP_MULTIPLIER = 2.5;

export function computeFuelPlan(input: FuelCalcInput): FuelCalcResult | null {
  const raceDurationS = raceDurationSeconds(input.hours, input.minutes);
  const lapTimeS = lapTimeFromParts(
    input.lapMinutes,
    input.lapSeconds,
    input.lapMilliseconds,
  );
  const fuelPerLap = input.fuelPerLapL;

  if (lapTimeS == null || lapTimeS <= 0 || fuelPerLap < 0 || !Number.isFinite(fuelPerLap)) {
    return null;
  }
  if (raceDurationS <= 0) {
    return { laps: 0, baseFuelL: 0, marginFuelL: 0, totalFuelL: 0 };
  }

  const laps = Math.ceil(raceDurationS / lapTimeS);
  const baseFuelL = laps * fuelPerLap;
  const marginFuelL = input.safetyMargin
    ? SAFETY_MARGIN_LAP_MULTIPLIER * fuelPerLap
    : 0;
  const totalFuelL = baseFuelL + marginFuelL;

  return { laps, baseFuelL, marginFuelL, totalFuelL };
}

export function formatFuelLiters(value: number): string {
  return `${value.toFixed(2)} L`;
}
