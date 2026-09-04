import { describe, expect, it } from "vitest";
import type { DistanceSample } from "../types";
import { buildTimeDeltaSeries } from "./chartAlign";
import { computeSegmentDeltas, computeSegmentTimes } from "./segments";

/** An even lap: `lapTimeS` spread over 400 points of the distance grid. */
function evenLap(lapTimeS: number, points = 400): DistanceSample[] {
  return Array.from({ length: points }, (_, i) => {
    const distance_pct = (i / (points - 1)) * 100;
    return sample(distance_pct, (distance_pct / 100) * lapTimeS);
  });
}

function sample(distance_pct: number, lap_time_s: number): DistanceSample {
  return {
    distance_pct,
    lap_time_s,
    speed_mps: 50,
    throttle: 1,
    brake: 0,
    steering: 0,
    gear: 4,
    rpm: 7000,
    pos_x: 0,
    pos_y: 0,
    pos_z: 0,
  };
}

describe("computeSegmentTimes", () => {
  it("splits a lap into ten windows", () => {
    const times = computeSegmentTimes(evenLap(90));
    expect(times).toHaveLength(10);
    for (const t of times) expect(t).toBeCloseTo(9, 1);
  });

  it("measures a lap whose last samples cross the start/finish line", () => {
    // The recording runs until the sim reports the crossing, so its final
    // samples come back with the timer already restarted - and the grid
    // interpolates between the two clocks on the way. Ordinary laps look like
    // this; the segments in front of the line are all perfectly measurable.
    const lap = evenLap(90);
    lap[lap.length - 2] = sample(lap[lap.length - 2].distance_pct, 23.4);
    lap[lap.length - 1] = sample(100, 0.017);

    const times = computeSegmentTimes(lap);
    expect(times.every((t) => t != null)).toBe(true);
    const total = (times as number[]).reduce((a, b) => a + b, 0);
    expect(total).toBeCloseTo(90, 0);
  });

  it("cannot measure a trace that spans several lap timers", () => {
    // Merged recording: the timer restarts partway round, so no window of these
    // samples measures a segment of one lap.
    const lap = evenLap(90);
    const restarted = lap.map((s, i) =>
      i > lap.length / 2 ? sample(s.distance_pct, s.lap_time_s - 45) : s,
    );
    expect(computeSegmentTimes(restarted)).toEqual(
      Array.from({ length: 10 }, () => null),
    );
  });

  it("draws no cells at all when there are no samples", () => {
    expect(computeSegmentTimes([])).toEqual([]);
  });
});

describe("computeSegmentDeltas", () => {
  it("reads compare minus reference", () => {
    const deltas = computeSegmentDeltas(evenLap(90), evenLap(100));
    for (const d of deltas) expect(d).toBeCloseTo(1, 1);
  });

  it("leaves a segment unmeasured when either lap could not measure it", () => {
    const restarted = evenLap(90).map((s, i, all) =>
      i > all.length / 2 ? sample(s.distance_pct, s.lap_time_s - 45) : s,
    );
    expect(computeSegmentDeltas(evenLap(90), restarted)).toEqual(
      Array.from({ length: 10 }, () => null),
    );
  });
});

describe("the readouts of one lap", () => {
  /**
   * A recording of `lapTimeS` that only starts `headS` past the line - its first
   * poll landed there - stretched over the distance grid the way the backend
   * resamples it.
   */
  function recordedFrom(
    headS: number,
    lapTimeS: number,
    points = 400,
  ): DistanceSample[] {
    return Array.from({ length: points }, (_, i) => {
      const distance_pct = (i / (points - 1)) * 100;
      return sample(
        distance_pct,
        headS + (distance_pct / 100) * (lapTimeS - headS),
      );
    });
  }

  it("account for the lap from the line, not from the first sample", () => {
    // The opening stretch is time the driver spent on the lap, and the sim's
    // clock already counts it. Dropping it would make the segments sum to less
    // than the lap took - by a different amount on every lap, since where the
    // first poll lands is chance.
    const head = 0.4;
    const times = computeSegmentTimes(recordedFrom(head, 90));
    const total = (times as number[]).reduce((a, b) => a + b, 0);
    expect(total).toBeCloseTo(90, 3);
    // The grid spreads the recorded part of the lap over all ten windows, so
    // the opening stretch lands on top of the first one.
    expect(times[0]).toBeCloseTo(head + (90 - head) / 10, 3);
    expect(times[1]).toBeCloseTo((90 - head) / 10, 3);
  });

  it("sum to the delta the full-lap chart ends on", () => {
    // Two recordings that caught the line at different moments. The strip and
    // the delta chart are read side by side, and a driver comparing them should
    // not have to reconcile two different totals - let alone two signs.
    const lap = recordedFrom(0.4, 90.1);
    const reference = recordedFrom(0.1, 90);

    const stripTotal = (
      computeSegmentDeltas(reference, lap) as number[]
    ).reduce((a, b) => a + b, 0);
    const chart = buildTimeDeltaSeries(lap, reference);

    expect(stripTotal).toBeCloseTo(0.1, 3);
    expect(stripTotal).toBeCloseTo(chart[chart.length - 1].lap_time_s, 3);
  });
});
