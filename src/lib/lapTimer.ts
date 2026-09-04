import type { DistanceSample } from "../types";

/**
 * A lap timer that runs backwards along the lap means the samples span a
 * start/finish crossing, so they cover more than one lap. Well above resampling
 * jitter, far below a real restart.
 */
const LAP_TIME_RESET_DROP_S = 1;

/**
 * How much of the lap's tail may carry the next lap's timer.
 *
 * A recording runs until the sim reports the crossing, so its last poll lands
 * just past the line - on ACC, up to the ~180 ms the adapter holds telemetry
 * back while it waits for the sim to time that crossing. Those samples arrive
 * with the timer already restarted, and the resampled grid then interpolates
 * between the two clocks, so the final points of a perfectly ordinary lap read
 * as a reset. That is the crossing itself, not a trace spanning laps: the whole
 * lap in front of it is timed against one clock.
 *
 * Wide enough for the crossing at every sample rate, far too narrow to hide a
 * recording that spans whole laps - those restart mid-lap and are caught.
 */
const CROSSING_TAIL_PCT = 1;

/**
 * The samples timed against this lap's clock: all of them when the timer never
 * restarts, or those before a restart on the tail, which is the start/finish
 * crossing the recording ran into.
 *
 * Null when the restart lands inside the lap instead. Those samples span more
 * than one lap timer, and nothing read off their clock measures this lap:
 * callers either say so or leave the trace alone, but must not treat what comes
 * back as one lap's timing.
 */
export function samplesBeforeLapTimerReset(
  samples: DistanceSample[],
): DistanceSample[] | null {
  const sorted = inDistanceOrder(samples);
  const reset = sorted.findIndex(
    (sample, i) =>
      i > 0 &&
      sorted[i - 1].lap_time_s - sample.lap_time_s > LAP_TIME_RESET_DROP_S,
  );
  if (reset === -1) return sorted;
  if (sorted[reset].distance_pct < 100 - CROSSING_TAIL_PCT) return null;
  const kept = sorted.slice(0, reset);
  return kept.length >= 2 ? kept : null;
}

function inDistanceOrder(samples: DistanceSample[]): DistanceSample[] {
  const ordered = samples.every(
    (s, i) => i === 0 || s.distance_pct >= samples[i - 1].distance_pct,
  );
  return ordered
    ? samples
    : [...samples].sort((a, b) => a.distance_pct - b.distance_pct);
}
