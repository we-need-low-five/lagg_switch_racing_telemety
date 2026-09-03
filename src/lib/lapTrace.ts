import type { LapRecord } from "../types";

/**
 * How much of a lap its trace has to cover before it counts as a whole
 * recording. Mirrors `COMPLETE_TRACE_COVERAGE` in `sim-core`, which is what
 * measured the number stored on the lap.
 */
export const COMPLETE_TRACE_COVERAGE = 0.98;

/**
 * Whether this lap's recording is a fragment of the lap rather than the whole
 * of it — the recorder attached part-way round, the sim froze mid-lap, or the
 * trace stops before the line.
 *
 * Such a lap is unusable and stored invalid, so this is what says *why* it is
 * invalid. A lap the sim scored invalid for a cut still has a whole trace and
 * overlays on a clean lap perfectly well; a fragment does not, because
 * `resample_to_distance_grid` normalises every lap onto 0-100 % of its *own*
 * trace and so stretches the fragment across the full width of the chart.
 *
 * A lap with no measure — recorded before it existed, and with no trace left to
 * backfill from — is treated as whole. It is kept either way, and throwing out
 * laps for a fault the measure could not see is worse than missing one it
 * could.
 */
export function isPartialTrace(
  lap: Pick<LapRecord, "trace_coverage">,
): boolean {
  const coverage = lap.trace_coverage;
  return typeof coverage === "number" && coverage < COMPLETE_TRACE_COVERAGE;
}

/** Laps on this list whose recording is a fragment. */
export function findPartialTraces<
  T extends { id: string; trace_coverage?: number | null },
>(laps: T[]): Set<string> {
  return new Set(laps.filter(isPartialTrace).map((lap) => lap.id));
}

/** Coverage for a warning line — "48 %", "6 %". */
export function formatCoveragePct(coverage: number): string {
  return `${Math.round(coverage * 100)} %`;
}
