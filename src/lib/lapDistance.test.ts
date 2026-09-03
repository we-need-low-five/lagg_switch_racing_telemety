import { describe, expect, it } from "vitest";
import type { DistanceSample } from "../types";
import {
  findLapRouteMismatch,
  findShortLaps,
  formatDistanceKm,
  lapDrivenDistanceM,
} from "./lapDistance";

/** A lap as the tables see it: an id and whatever distance was measured. */
function lap(id: string, lapDistanceM: number | null | undefined) {
  return { id, lap_distance_m: lapDistanceM };
}

/** A resampled trace running straight along X, `step` metres between points. */
function grid(points: number, step: number): DistanceSample[] {
  return Array.from(
    { length: points },
    (_, i) => ({ pos_x: i * step, pos_y: 0, pos_z: 0 }) as DistanceSample,
  );
}

// Real Nurburgring 24h figures: the full lap is 25.4 km, a joker lap round the
// GP loop alone is 4.6 km.
const FULL_M = 25_400;
const JOKER_M = 4_600;

describe("findShortLaps", () => {
  it("marks joker laps among full ones", () => {
    const short = findShortLaps([
      lap("a", FULL_M),
      lap("b", 25_380),
      lap("c", JOKER_M),
      lap("d", 25_410),
      lap("e", 4_550),
    ]);
    expect([...short].sort()).toEqual(["c", "e"]);
  });

  it("leaves the honest spread between full laps alone", () => {
    // Traffic and a spin cost time, not ground.
    const short = findShortLaps([
      lap("a", FULL_M),
      lap("b", 25_100),
      lap("c", 24_900),
    ]);
    expect(short.size).toBe(0);
  });

  it("marks nothing when there is no full lap to measure against", () => {
    const short = findShortLaps([lap("c", JOKER_M), lap("e", 4_550)]);
    expect(short.size).toBe(0);
  });

  it("settles the earlier laps once a full one is recorded", () => {
    const short = findShortLaps([
      lap("c", JOKER_M),
      lap("e", 4_550),
      lap("a", FULL_M),
    ]);
    expect([...short].sort()).toEqual(["c", "e"]);
  });

  it("needs two measured laps before it marks anything", () => {
    expect(findShortLaps([lap("a", FULL_M)]).size).toBe(0);
  });

  it("ignores laps that were never measured", () => {
    // Unmeasured is unknown, not short — a legacy row or a trace with no
    // usable positions must not be called the shortest lap on the track.
    const short = findShortLaps([
      lap("a", FULL_M),
      lap("b", null),
      lap("c", undefined),
      lap("d", 0),
    ]);
    expect(short.size).toBe(0);
  });

  it("puts the fence at 10 % short of the longest lap", () => {
    expect(findShortLaps([lap("a", 10_000), lap("b", 9_000)]).size).toBe(0);
    expect([...findShortLaps([lap("a", 10_000), lap("b", 8_999)])]).toEqual([
      "b",
    ]);
  });
});

describe("lapDrivenDistanceM", () => {
  it("sums the ground between points", () => {
    expect(lapDrivenDistanceM(grid(100, 10))).toBeCloseTo(990, 3);
  });

  it("is zero for a trace with no positions to speak of", () => {
    expect(lapDrivenDistanceM(grid(100, 0))).toBe(0);
    expect(lapDrivenDistanceM([])).toBe(0);
  });
});

describe("findLapRouteMismatch", () => {
  it("catches a joker lap overlaid on a full one", () => {
    const mismatch = findLapRouteMismatch(["full", "joker"], {
      full: grid(100, FULL_M / 99),
      joker: grid(100, JOKER_M / 99),
    });
    expect(mismatch?.short.map((s) => s.lapId)).toEqual(["joker"]);
    expect(mismatch?.longestM).toBeCloseTo(FULL_M, 0);
  });

  it("stays quiet when both laps went the same way round", () => {
    expect(
      findLapRouteMismatch(["a", "b"], {
        full: grid(100, 254),
        a: grid(100, 254),
        b: grid(100, 252),
      }),
    ).toBeNull();
  });

  it("reports the shortest lap first", () => {
    const mismatch = findLapRouteMismatch(["full", "short", "shorter"], {
      full: grid(100, FULL_M / 99),
      short: grid(100, 10_000 / 99),
      shorter: grid(100, JOKER_M / 99),
    });
    expect(mismatch?.short.map((s) => s.lapId)).toEqual(["shorter", "short"]);
  });

  it("needs two laps with usable geometry", () => {
    // One lap resampled off `distance_m` leaves nothing to compare against.
    expect(
      findLapRouteMismatch(["a", "b"], {
        a: grid(100, 254),
        b: grid(100, 0),
      }),
    ).toBeNull();
    expect(findLapRouteMismatch(["a"], { a: grid(100, 254) })).toBeNull();
  });
});

describe("formatDistanceKm", () => {
  it("reads as kilometres", () => {
    expect(formatDistanceKm(FULL_M)).toBe("25.4 km");
    expect(formatDistanceKm(JOKER_M)).toBe("4.6 km");
  });
});
