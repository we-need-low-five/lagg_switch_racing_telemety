import type { DistanceSample } from "../types";
import { interpolateSampleAtPct } from "./chartAlign";
import { samplesBeforeLapTimerReset } from "./lapTimer";

export const SEGMENT_COUNT = 10;

export type SegmentTab = "full" | number;

export interface DistanceRange {
  id: SegmentTab;
  label: string;
  start_pct: number;
  end_pct: number;
}

export function getSegmentRanges(): DistanceRange[] {
  return Array.from({ length: SEGMENT_COUNT }, (_, i) => ({
    id: i,
    label: `S${i + 1}`,
    start_pct: i * 10,
    end_pct: (i + 1) * 10,
  }));
}

export function getActiveSegmentRange(
  segmentTab: SegmentTab,
  ranges: DistanceRange[],
): DistanceRange | null {
  if (segmentTab === "full") return null;
  return ranges.find((r) => r.id === segmentTab) ?? null;
}

export function filterSamplesToRange(
  samples: DistanceSample[],
  range: DistanceRange,
): DistanceSample[] {
  const span = range.end_pct - range.start_pct || 1;
  return samples
    .filter(
      (s) =>
        s.distance_pct >= range.start_pct - 0.05 &&
        s.distance_pct <= range.end_pct + 0.05,
    )
    .map((s) => ({
      ...s,
      distance_pct: ((s.distance_pct - range.start_pct) / span) * 100,
    }));
}

export function mapCursorToRangeLocal(
  lapPct: number | null,
  segmentTab: SegmentTab,
  ranges: DistanceRange[],
): number | null {
  if (lapPct == null) return null;
  if (segmentTab === "full") return lapPct;
  const range = ranges.find((r) => r.id === segmentTab);
  if (!range) return lapPct;
  const span = range.end_pct - range.start_pct || 1;
  if (lapPct < range.start_pct - 0.05 || lapPct > range.end_pct + 0.05) {
    return null;
  }
  return ((lapPct - range.start_pct) / span) * 100;
}

export function mapRangeLocalToLapPct(
  localPct: number | null,
  segmentTab: SegmentTab,
  ranges: DistanceRange[],
): number | null {
  if (localPct == null) return null;
  if (segmentTab === "full") return localPct;
  const range = ranges.find((r) => r.id === segmentTab);
  if (!range) return localPct;
  const span = range.end_pct - range.start_pct || 1;
  return range.start_pct + (localPct / 100) * span;
}

/**
 * Elapsed lap time at each 10% distance boundary (0, 10, …, 100), or null when
 * the samples span more than one lap.
 *
 * Read off the sim's own lap clock, which every adapter fills from the timer
 * that resets at the start/finish line - so the first boundary is zero, the
 * line itself, and not the first sample. A recording's first poll lands a
 * fraction of a second past the line and each lap's is at a different point, so
 * measuring from it would drop that opening stretch out of the lap: the
 * segments would sum to less than the lap took, by a different amount on each
 * lap, and comparing two of them would credit one for time the other was never
 * charged. The opening stretch belongs to the first segment, where the clock
 * puts it.
 */
export function elapsedAtBoundaries(
  samples: DistanceSample[],
): number[] | null {
  const prepared = prepareMonotonicLapTimes(samples);
  if (prepared == null) return null;
  const boundaries = Array.from({ length: SEGMENT_COUNT + 1 }, (_, i) => i * 10);
  return boundaries.map((pct, i) => {
    if (i === 0) return 0;
    const sample = interpolateSampleAtPct(prepared, pct);
    return sample?.lap_time_s ?? 0;
  });
}

/**
 * Per-segment elapsed times (10 windows of ~10% distance each). Empty when
 * there are no samples at all; a segment is null when it cannot be measured.
 */
export function computeSegmentTimes(
  samples: DistanceSample[],
): (number | null)[] {
  if (samples.length === 0) return [];
  const boundaries = elapsedAtBoundaries(samples);
  if (boundaries == null) {
    return Array.from({ length: SEGMENT_COUNT }, () => null);
  }
  return Array.from({ length: SEGMENT_COUNT }, (_, i) => {
    const duration = boundaries[i + 1] - boundaries[i];
    if (!Number.isFinite(duration) || duration < 0) return null;
    return duration;
  });
}

/**
 * Samples ordered by distance, on the sim's lap clock, with the timer forced
 * non-decreasing to absorb resampling jitter. The tail past a start/finish
 * crossing is dropped, which leaves the last boundary read off the final sample
 * before the line - short of 100% by a sample or two. Null when the timer
 * restarts partway, where no window of the samples measures a segment: the
 * running maximum would flatten whole stretches to a standstill and report them
 * as segments of 0s.
 */
function prepareMonotonicLapTimes(
  samples: DistanceSample[],
): DistanceSample[] | null {
  const beforeReset = samplesBeforeLapTimerReset(samples);
  if (beforeReset == null) return null;
  let maxTime = 0;
  const monotonic: DistanceSample[] = [];
  for (const sample of beforeReset) {
    maxTime = Math.max(maxTime, sample.lap_time_s);
    monotonic.push({ ...sample, lap_time_s: maxTime });
  }
  return monotonic;
}

/**
 * Segment deltas in seconds (compare − reference); negative = faster. Null
 * where either lap could not measure that segment.
 */
export function computeSegmentDeltas(
  reference: DistanceSample[],
  compare: DistanceSample[],
): (number | null)[] {
  const refTimes = computeSegmentTimes(reference);
  const cmpTimes = computeSegmentTimes(compare);
  if (refTimes.length === 0 || cmpTimes.length === 0) return [];
  return refTimes.map((ref, i) => {
    const cmp = cmpTimes[i];
    return ref == null || cmp == null ? null : cmp - ref;
  });
}
