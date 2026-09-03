import { describe, expect, it } from "vitest";
import type { LapRecord } from "../types";
import {
  averageExcludingExtremeOutliers,
  computeSessionLapStats,
} from "./sessionLapStats";

function lap(
  lapNumber: number,
  lapTimeMs: number,
  valid = true,
  extra: Partial<LapRecord> = {},
): LapRecord {
  return {
    id: `lap-${lapNumber}`,
    session_id: "session",
    lap_number: lapNumber,
    lap_time_ms: lapTimeMs,
    valid,
    is_best: false,
    is_pinned: false,
    sectors: {},
    sample_rate_hz: 100,
    ...extra,
  };
}

/** Nurburgring 24h: full laps around 8:30, a joker round the GP loop at 2:30. */
const FULL_MS = 510_000;
const JOKER_MS = 150_000;

describe("computeSessionLapStats", () => {
  it("keeps a joker lap out of the average", () => {
    const laps = [
      lap(1, FULL_MS),
      lap(2, 512_000),
      lap(3, JOKER_MS, false),
      lap(4, 508_000),
      lap(5, 515_000),
    ];
    const stats = computeSessionLapStats(laps);

    // The plain mean of all five is a lap time nobody drove.
    const naive = Math.round(
      laps.reduce((sum, l) => sum + l.lap_time_ms, 0) / laps.length,
    );
    expect(naive).toBeLessThan(450_000);

    expect(stats.averageLapMs).toBeGreaterThan(500_000);
    expect(stats.averageLapMs).toBeLessThan(520_000);
    expect(stats.averageLapCount).toBe(4);
  });

  it("still counts the in-lap and out-lap it was always meant to", () => {
    // Slower laps are ordinary running, not outliers — the card says "average
    // lap", and the neighbouring one says "average valid lap".
    const laps = [
      lap(1, 540_000, false),
      lap(2, FULL_MS),
      lap(3, 512_000),
      lap(4, 508_000),
      lap(5, 560_000, false),
    ];
    const stats = computeSessionLapStats(laps);
    expect(stats.averageLapCount).toBe(5);
  });

  it("reports no average for a session with no laps", () => {
    const stats = computeSessionLapStats([]);
    expect(stats.averageLapMs).toBeNull();
    expect(stats.averageLapCount).toBe(0);
  });

  it("cannot fence a handful of laps, so it keeps them all", () => {
    // Under four values the IQR fences are unreliable and everything is kept —
    // a three-lap session containing a joker still skews.
    const stats = computeSessionLapStats([
      lap(1, FULL_MS),
      lap(2, 512_000),
      lap(3, JOKER_MS, false),
    ]);
    expect(stats.averageLapCount).toBe(3);
    expect(stats.averageLapMs).toBeLessThan(400_000);
  });

  it("leaves the valid-lap average to the laps ACC kept", () => {
    const stats = computeSessionLapStats([
      lap(1, FULL_MS),
      lap(2, 512_000),
      lap(3, JOKER_MS, false),
      lap(4, 508_000),
    ]);
    expect(stats.validLapCount).toBe(3);
    expect(stats.averageValidLapMs).toBe(510_000);
  });
});

describe("averageExcludingExtremeOutliers", () => {
  it("returns nothing for no values", () => {
    expect(averageExcludingExtremeOutliers([])).toEqual({
      average: null,
      usedCount: 0,
    });
  });

  it("keeps every value when they are all alike", () => {
    const { average, usedCount } = averageExcludingExtremeOutliers([
      10, 10, 10, 10,
    ]);
    expect(average).toBe(10);
    expect(usedCount).toBe(4);
  });
});
