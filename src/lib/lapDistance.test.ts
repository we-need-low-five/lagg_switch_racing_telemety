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

/** A 100-point trace covering `metres` end to end. */
function trace(metres: number): DistanceSample[] {
  return grid(100, metres / 99);
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

  it("is not thrown by an out-lap running longer than a flying lap", () => {
    // Measured off a real Red Bull Ring session. The out-lap drives the pit
    // lane as well as the track, so it is the longest lap there — taking the
    // longest as the reference marked thirteen of these sixteen short.
    const distances = [
      4791, 4546, 4303, 4335, 4287, 4280, 4282, 4281, 4285, 4284, 4286, 4283,
      4287, 4284, 4284, 4278,
    ];
    const short = findShortLaps(distances.map((m, i) => lap(String(i), m)));
    expect([...short]).toEqual([]);
  });

  it("is not thrown by a recording that merged several crossings", () => {
    // The 24h session that started this: lap 2's parquet spans four lap timers
    // and 44 km of a 25 km circuit. Only the 4.6 km joker is short.
    const short = findShortLaps([
      lap("joker", 4_648),
      lap("merged", 44_068),
      lap("full", 25_156),
    ]);
    expect([...short]).toEqual(["joker"]);
  });

  it("marks nothing when short laps are most of the session", () => {
    // With no full lap to speak of there is no route to be short of. Failing
    // this way costs a missing mark, not a table full of wrong ones.
    const short = findShortLaps([
      lap("c", JOKER_M),
      lap("e", 4_550),
      lap("a", FULL_M),
    ]);
    expect(short.size).toBe(0);
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

  it("puts the fence at 10 % short of a lap of the route", () => {
    expect(
      findShortLaps([lap("a", 10_000), lap("b", 10_000), lap("c", 9_000)]).size,
    ).toBe(0);
    expect([
      ...findShortLaps([lap("a", 10_000), lap("b", 10_000), lap("c", 8_999)]),
    ]).toEqual(["c"]);
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
      full: trace(FULL_M),
      joker: trace(JOKER_M),
    });
    expect(mismatch?.short.map((s) => s.lapId)).toEqual(["joker"]);
    expect(mismatch?.referenceM).toBeCloseTo((FULL_M + JOKER_M) / 2, 0);
  });

  it("stays quiet when both laps went the same way round", () => {
    expect(
      findLapRouteMismatch(["a", "b"], {
        a: trace(25_146),
        b: trace(24_948),
      }),
    ).toBeNull();
  });

  it("does not warn when one of the two laps is just an out-lap", () => {
    // The Red Bull Ring pair: 4791 m out-lap against a 4283 m flying lap.
    expect(
      findLapRouteMismatch(["out", "flying"], {
        out: trace(4_791),
        flying: trace(4_283),
      }),
    ).toBeNull();
  });

  it("reports the shortest lap first", () => {
    const mismatch = findLapRouteMismatch(
      ["full", "alsoFull", "short", "shorter"],
      {
        full: trace(FULL_M),
        alsoFull: trace(25_300),
        short: trace(10_000),
        shorter: trace(JOKER_M),
      },
    );
    expect(mismatch?.short.map((s) => s.lapId)).toEqual(["shorter", "short"]);
  });

  it("needs two laps with usable geometry", () => {
    // One lap resampled off `distance_m` leaves nothing to compare against.
    expect(
      findLapRouteMismatch(["a", "b"], {
        a: trace(25_146),
        b: grid(100, 0),
      }),
    ).toBeNull();
    expect(findLapRouteMismatch(["a"], { a: trace(25_146) })).toBeNull();
  });
});

describe("formatDistanceKm", () => {
  it("reads as kilometres", () => {
    expect(formatDistanceKm(FULL_M)).toBe("25.4 km");
    expect(formatDistanceKm(JOKER_M)).toBe("4.6 km");
  });
});
