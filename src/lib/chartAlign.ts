import type uPlot from "uplot";
import type { DistanceSample } from "../types";
import type { DistanceRange } from "./segments";
import { filterSamplesToRange } from "./segments";
import { samplesBeforeLapTimerReset } from "./lapTimer";

export const DISTANCE_GRID_POINTS = 4000;

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** Standard 0–100% distance grid (matches backend resample). */
export function standardDistanceGrid(
  pointCount = DISTANCE_GRID_POINTS,
): number[] {
  if (pointCount <= 1) return [0];
  return Array.from(
    { length: pointCount },
    (_, i) => (i / (pointCount - 1)) * 100,
  );
}

export function interpolateSampleAtPct(
  samples: DistanceSample[],
  targetPct: number,
): DistanceSample | null {
  if (samples.length === 0) return null;
  const sorted =
    samples.length > 1 &&
    samples.some((s, i) => i > 0 && s.distance_pct < samples[i - 1].distance_pct)
      ? [...samples].sort((a, b) => a.distance_pct - b.distance_pct)
      : samples;
  if (targetPct <= sorted[0].distance_pct) return sorted[0];
  const last = sorted[sorted.length - 1];
  if (targetPct >= last.distance_pct) return last;

  let lo = 0;
  let hi = sorted.length - 1;
  while (lo < hi - 1) {
    const mid = Math.floor((lo + hi) / 2);
    if (sorted[mid].distance_pct <= targetPct) lo = mid;
    else hi = mid;
  }

  const a = sorted[lo];
  const b = sorted[hi];
  const span = b.distance_pct - a.distance_pct;
  const t = span === 0 ? 0 : (targetPct - a.distance_pct) / span;

  return {
    distance_pct: targetPct,
    lap_time_s: lerp(a.lap_time_s, b.lap_time_s, t),
    speed_mps: lerp(a.speed_mps, b.speed_mps, t),
    throttle: lerp(a.throttle, b.throttle, t),
    brake: lerp(a.brake, b.brake, t),
    steering: lerp(a.steering, b.steering, t),
    gear: t < 1 ? a.gear : b.gear,
    rpm: lerp(a.rpm, b.rpm, t),
    pos_x: lerp(a.pos_x, b.pos_x, t),
    pos_y: lerp(a.pos_y, b.pos_y, t),
    pos_z: lerp(a.pos_z, b.pos_z, t),
  };
}

export function resampleLapToGrid(
  samples: DistanceSample[],
  gridPct: number[],
): DistanceSample[] {
  if (samples.length === 0) return [];
  return gridPct.map((pct) => {
    const sample = interpolateSampleAtPct(samples, pct);
    return sample ?? { ...samples[0], distance_pct: pct };
  });
}

export function lapsShareDistanceGrid(samples: DistanceSample[][]): boolean {
  const loaded = samples.filter((s) => s.length > 0);
  if (loaded.length === 0) return false;
  const primary = loaded[0];
  return loaded.every(
    (lap) =>
      lap.length === primary.length &&
      lap.every(
        (s, i) => Math.abs(s.distance_pct - primary[i].distance_pct) < 0.05,
      ),
  );
}

/** Align laps onto one distance % axis for uPlot (all Y series must match X length). */
export function alignLapSamples(
  samplesList: DistanceSample[][],
  grid?: number[],
): { x: number[]; aligned: DistanceSample[][] } {
  const nonEmpty = samplesList.filter((s) => s.length > 0);
  if (nonEmpty.length === 0) return { x: [], aligned: [] };

  if (lapsShareDistanceGrid(nonEmpty)) {
    const primary = nonEmpty[0];
    const x = primary.map((s) => s.distance_pct);
    return {
      x,
      aligned: samplesList.map((lap) =>
        lap.length > 0 ? lap : resampleLapToGrid(lap, x),
      ),
    };
  }

  const x =
    grid ??
    standardDistanceGrid(
      Math.max(
        DISTANCE_GRID_POINTS,
        ...nonEmpty.map((s) => s.length),
      ),
    );

  return {
    x,
    aligned: samplesList.map((lap) =>
      lap.length > 0 ? resampleLapToGrid(lap, x) : [],
    ),
  };
}

export function lapTimeDeltaAtPct(
  lapSample: DistanceSample,
  reference: DistanceSample[],
): number {
  const ref = interpolateSampleAtPct(reference, lapSample.distance_pct);
  if (!ref) return 0;
  return lapSample.lap_time_s - ref.lap_time_s;
}

/**
 * Time delta vs reference along lap distance. For a sector range, filters to that
 * sector and zeroes delta at sector entry so the chart shows gain/loss within the sector.
 *
 * Both traces are cut at the start/finish crossing first. A recording keeps the
 * poll that lands past the line, where the sim has already restarted the clock,
 * and subtracting one lap's opening milliseconds from another's finishing
 * minutes puts a whole lap time into the last points of the series - a spike
 * that takes the chart's Y scale with it and flattens the delta everyone came
 * to read. A trace that spans several lap timers is left as it is: there is no
 * one crossing to cut at, and the segment strip already reports it unmeasurable.
 */
export function buildTimeDeltaSeries(
  lapSamples: DistanceSample[],
  reference: DistanceSample[],
  range?: DistanceRange | null,
): DistanceSample[] {
  if (lapSamples.length === 0 || reference.length === 0) return [];

  const lap = samplesBeforeLapTimerReset(lapSamples) ?? lapSamples;
  const timed = samplesBeforeLapTimerReset(reference) ?? reference;

  const fullDelta = lap.map((s) => ({
    ...s,
    lap_time_s: lapTimeDeltaAtPct(s, timed),
  }));

  if (!range) return fullDelta;

  const segmentDelta = filterSamplesToRange(fullDelta, range);
  if (segmentDelta.length === 0) return [];

  const base = segmentDelta[0].lap_time_s;
  return segmentDelta.map((s) => ({
    ...s,
    lap_time_s: s.lap_time_s - base,
  }));
}

/** Move uPlot crosshair without firing setCursor hooks (for track-map driven sync). */
export function applyCursorToPlot(plot: uPlot, cursorPct: number | null): void {
  if (cursorPct == null) {
    plot.setCursor({ left: -10, top: -10 }, false);
    return;
  }
  const xData = plot.data[0];
  if (xData.length === 0) return;
  let nearest = 0;
  let best = Infinity;
  for (let i = 0; i < xData.length; i += 1) {
    const d = Math.abs(xData[i] - cursorPct);
    if (d < best) {
      best = d;
      nearest = i;
    }
  }
  const left = plot.valToPos(xData[nearest], "x");
  const top = plot.cursor.top ?? -10;
  plot.setCursor({ left, top }, false);
}
