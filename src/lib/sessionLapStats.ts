import type { GameId, LapRecord } from "../types";
import { displaySectorTimes } from "./compareLaps";

export interface SessionLapStats {
  bestLap: LapRecord | null;
  /** Fastest S1/S2/S3 among valid laps (ms). */
  bestS1Ms: number | null;
  bestS2Ms: number | null;
  bestS3Ms: number | null;
  optimalLapMs: number | null;
  averageLapMs: number | null;
  averageValidLapMs: number | null;
  validLapCount: number;
  /** Mean fuel used per valid lap (L), after dropping extreme IQR outliers. */
  averageFuelL: number | null;
  /** Valid laps included in averageFuelL (finite fuel readings after outlier filter). */
  averageFuelLapCount: number;
}

/** Tukey extreme-outlier fence multiplier (beyond 3×IQR). */
const EXTREME_OUTLIER_IQR_FACTOR = 3;

function quantileSorted(sorted: number[], q: number): number {
  if (sorted.length === 1) return sorted[0];
  const pos = (sorted.length - 1) * q;
  const lo = Math.floor(pos);
  const hi = Math.ceil(pos);
  if (lo === hi) return sorted[lo];
  const t = pos - lo;
  return sorted[lo] * (1 - t) + sorted[hi] * t;
}

/**
 * Arithmetic mean after excluding Tukey extreme outliers (outside Q1−3·IQR … Q3+3·IQR).
 * With fewer than 4 samples, IQR fences are unreliable so all values are kept.
 */
export function averageExcludingExtremeOutliers(
  values: number[],
): { average: number | null; usedCount: number } {
  if (values.length === 0) {
    return { average: null, usedCount: 0 };
  }

  let used = values;
  if (values.length >= 4) {
    const sorted = [...values].sort((a, b) => a - b);
    const q1 = quantileSorted(sorted, 0.25);
    const q3 = quantileSorted(sorted, 0.75);
    const iqr = q3 - q1;
    if (iqr > 0) {
      const lower = q1 - EXTREME_OUTLIER_IQR_FACTOR * iqr;
      const upper = q3 + EXTREME_OUTLIER_IQR_FACTOR * iqr;
      const filtered = values.filter((v) => v >= lower && v <= upper);
      if (filtered.length > 0) used = filtered;
    }
  }

  const average = used.reduce((sum, v) => sum + v, 0) / used.length;
  return { average, usedCount: used.length };
}

export function computeSessionLapStats(laps: LapRecord[], game?: GameId): SessionLapStats {
  if (laps.length === 0) {
    return {
      bestLap: null,
      bestS1Ms: null,
      bestS2Ms: null,
      bestS3Ms: null,
      optimalLapMs: null,
      averageLapMs: null,
      averageValidLapMs: null,
      validLapCount: 0,
      averageFuelL: null,
      averageFuelLapCount: 0,
    };
  }

  const validLaps = laps.filter((lap) => lap.valid);
  const bestLap =
    laps.find((lap) => lap.is_best) ??
    (validLaps.length > 0
      ? validLaps.reduce((a, b) =>
          a.lap_time_ms <= b.lap_time_ms ? a : b,
        )
      : null);

  const averageLapMs = Math.round(
    laps.reduce((sum, lap) => sum + lap.lap_time_ms, 0) / laps.length,
  );

  const averageValidLapMs =
    validLaps.length > 0
      ? Math.round(
          validLaps.reduce((sum, lap) => sum + lap.lap_time_ms, 0) /
            validLaps.length,
        )
      : null;

  const sectorSource = validLaps.length > 0 ? validLaps : laps;
  let bestS1: number | null = null;
  let bestS2: number | null = null;
  let bestS3: number | null = null;

  for (const lap of sectorSource) {
    const sectors = displaySectorTimes(lap.sectors, lap.lap_time_ms, game);
    if (sectors.s1_ms != null) {
      bestS1 = bestS1 == null ? sectors.s1_ms : Math.min(bestS1, sectors.s1_ms);
    }
    if (sectors.s2_ms != null) {
      bestS2 = bestS2 == null ? sectors.s2_ms : Math.min(bestS2, sectors.s2_ms);
    }
    if (sectors.s3_ms != null) {
      bestS3 = bestS3 == null ? sectors.s3_ms : Math.min(bestS3, sectors.s3_ms);
    }
  }

  const optimalLapMs =
    bestS1 != null && bestS2 != null && bestS3 != null
      ? bestS1 + bestS2 + bestS3
      : null;

  const fuelValues = validLaps
    .map((lap) => lap.fuel_used_l)
    .filter((v): v is number => v != null && Number.isFinite(v) && v > 0);
  const fuelAvg = averageExcludingExtremeOutliers(fuelValues);

  return {
    bestLap,
    bestS1Ms: bestS1,
    bestS2Ms: bestS2,
    bestS3Ms: bestS3,
    optimalLapMs,
    averageLapMs,
    averageValidLapMs,
    validLapCount: validLaps.length,
    averageFuelL: fuelAvg.average,
    averageFuelLapCount: fuelAvg.usedCount,
  };
}
