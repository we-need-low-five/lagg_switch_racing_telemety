import type { GameId, LapRecord } from "../types";
import { displaySectorTimes } from "./compareLaps";

export interface SessionLapStats {
  bestLap: LapRecord | null;
  optimalLapMs: number | null;
  averageLapMs: number | null;
  averageValidLapMs: number | null;
  validLapCount: number;
}

export function computeSessionLapStats(laps: LapRecord[], game?: GameId): SessionLapStats {
  if (laps.length === 0) {
    return {
      bestLap: null,
      optimalLapMs: null,
      averageLapMs: null,
      averageValidLapMs: null,
      validLapCount: 0,
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

  return {
    bestLap,
    optimalLapMs,
    averageLapMs,
    averageValidLapMs,
    validLapCount: validLaps.length,
  };
}
