import type { DistanceSample } from "../types";
import { interpolateSampleAtPct } from "./chartAlign";

export const SEGMENT_COUNT = 10;

/**
 * A lap timer that runs backwards along the lap means the samples span a
 * start/finish crossing, so they cover more than one lap and no window of them
 * measures a segment. Well above resampling jitter, far below a real restart.
 */
const LAP_TIME_RESET_DROP_S = 1;

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
 */
export function elapsedAtBoundaries(
  samples: DistanceSample[],
): number[] | null {
  const prepared = prepareMonotonicLapTimes(samples);
  if (prepared == null) return null;
  const boundaries = Array.from({ length: SEGMENT_COUNT + 1 }, (_, i) => i * 10);
  return boundaries.map((pct) => {
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
 * Samples ordered by distance with the lap timer forced non-decreasing, which
 * absorbs resampling jitter, and rebased so the lap starts at zero. Null when
 * the timer restarts partway - the running maximum would flatten whole
 * stretches to a standstill and report them as segments of 0s.
 */
function prepareMonotonicLapTimes(
  samples: DistanceSample[],
): DistanceSample[] | null {
  const sorted = [...samples].sort((a, b) => a.distance_pct - b.distance_pct);
  let maxTime = 0;
  const monotonic: DistanceSample[] = [];
  for (let i = 0; i < sorted.length; i += 1) {
    const sample = sorted[i];
    if (
      i > 0 &&
      sorted[i - 1].lap_time_s - sample.lap_time_s > LAP_TIME_RESET_DROP_S
    ) {
      return null;
    }
    maxTime = Math.max(maxTime, sample.lap_time_s);
    monotonic.push({ ...sample, lap_time_s: maxTime });
  }
  const startTime =
    interpolateSampleAtPct(monotonic, 0)?.lap_time_s ??
    monotonic[0]?.lap_time_s ??
    0;
  return monotonic.map((sample) => ({
    ...sample,
    lap_time_s: Math.max(0, sample.lap_time_s - startTime),
  }));
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
