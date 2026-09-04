import { describe, expect, it } from "vitest";
import type { DistanceSample } from "../types";
import { buildTimeDeltaSeries } from "./chartAlign";

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

/** An even lap: `lapTimeS` spread over 400 points of the distance grid. */
function evenLap(lapTimeS: number, points = 400): DistanceSample[] {
  return Array.from({ length: points }, (_, i) => {
    const distance_pct = (i / (points - 1)) * 100;
    return sample(distance_pct, (distance_pct / 100) * lapTimeS);
  });
}

/**
 * The last polls of a recording, taken past the line on the next lap's clock.
 * With two, the grid point before the line is a blend of the two clocks - where
 * a recording's final poll falls is up to the sample rate, so two traces of the
 * same lap rarely cross it the same way.
 */
function crossingTail(lap: DistanceSample[], polls: 1 | 2): DistanceSample[] {
  const crossed = [...lap];
  if (polls === 2) {
    crossed[crossed.length - 2] = sample(
      crossed[crossed.length - 2].distance_pct,
      23.4,
    );
  }
  crossed[crossed.length - 1] = sample(100, 0.017);
  return crossed;
}

function maxAbsDelta(series: DistanceSample[]): number {
  return Math.max(...series.map((s) => Math.abs(s.lap_time_s)));
}

describe("buildTimeDeltaSeries", () => {
  it("reads the gap to the reference along the lap", () => {
    const series = buildTimeDeltaSeries(evenLap(91), evenLap(90));
    expect(series[series.length - 1].lap_time_s).toBeCloseTo(1, 3);
  });

  it("does not spike where the recording crosses the start/finish line", () => {
    // Both traces keep the poll that landed past the line, on a clock the sim
    // has already restarted. Subtracting one lap's opening milliseconds from
    // another's finishing minutes would put a whole lap time in the last points
    // and take the chart's Y scale with it.
    const series = buildTimeDeltaSeries(
      crossingTail(evenLap(91), 2),
      crossingTail(evenLap(90), 1),
    );
    expect(series.length).toBeGreaterThan(0);
    expect(maxAbsDelta(series)).toBeLessThan(2);
  });

  it("does not spike when only the reference crosses the line", () => {
    const series = buildTimeDeltaSeries(
      evenLap(91),
      crossingTail(evenLap(90), 1),
    );
    expect(maxAbsDelta(series)).toBeLessThan(2);
  });

  it("leaves a trace that spans several lap timers alone", () => {
    // No one crossing to cut at. The segment strip already reports such a lap
    // unmeasurable; throwing away half its delta line would say less, not more.
    const lap = evenLap(90);
    const restarted = lap.map((s, i) =>
      i > lap.length / 2 ? sample(s.distance_pct, s.lap_time_s - 45) : s,
    );
    expect(buildTimeDeltaSeries(restarted, evenLap(90))).toHaveLength(
      restarted.length,
    );
  });

  it("zeroes the delta at the entry to a segment", () => {
    const series = buildTimeDeltaSeries(evenLap(91), evenLap(90), {
      id: 5,
      label: "S6",
      start_pct: 50,
      end_pct: 60,
    });
    expect(series[0].lap_time_s).toBe(0);
    expect(series[series.length - 1].lap_time_s).toBeCloseTo(0.1, 2);
  });
});
