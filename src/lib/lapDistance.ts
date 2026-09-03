import type { DistanceSample } from "../types";

/**
 * How far a lap's driven distance may fall short of the longest lap on the
 * chart before the two are treated as different routes rather than two runs
 * of the same one. Traffic and a spin cost time, not ground, so the spread
 * between honest laps of one route is well inside this.
 */
export const LAP_ROUTE_TOLERANCE = 0.1;

/**
 * Metres of track a lap covered, summed from the world positions in its
 * resampled grid. Mirrors `build_cumulative_distance` in `sim-core`, which is
 * what the grid was laid out against in the first place.
 */
export function lapDrivenDistanceM(samples: DistanceSample[]): number {
  let total = 0;
  for (let i = 1; i < samples.length; i += 1) {
    const a = samples[i - 1];
    const b = samples[i];
    const dx = b.pos_x - a.pos_x;
    const dy = b.pos_y - a.pos_y;
    const dz = b.pos_z - a.pos_z;
    total += Math.sqrt(dx * dx + dy * dy + dz * dz);
  }
  return total;
}

export interface LapRouteMismatch {
  /** Driven distance of the longest lap on the chart, in metres. */
  longestM: number;
  /** Laps that covered materially less ground than that, shortest first. */
  short: { lapId: string; distanceM: number }[];
  distancesM: Map<string, number>;
}

/**
 * Laps on the chart that did not cover the same route, or `null` when they all
 * did.
 *
 * `resample_to_distance_grid` normalises every lap onto 0–100 % of *its own*
 * total distance, so a lap round part of the track is stretched across the
 * same axis as a full one. On the Nurburgring 24h layout a "joker" lap of the
 * GP loop alone is 4.6 km of a 25.4 km lap: ACC invalidates it like any other
 * cut, but it is still a lap in the list and still selectable here, and
 * overlaid on a full lap it produces a delta trace that looks reasonable and
 * means nothing.
 */
export function findLapRouteMismatch(
  lapIds: string[],
  samples: Record<string, DistanceSample[]>,
  tolerance = LAP_ROUTE_TOLERANCE,
): LapRouteMismatch | null {
  const distancesM = new Map<string, number>();
  for (const lapId of lapIds) {
    const grid = samples[lapId];
    if (!grid || grid.length < 2) continue;
    const distanceM = lapDrivenDistanceM(grid);
    // A grid with degenerate positions resamples off `distance_m` instead, so
    // it carries no usable geometry — leave it out rather than call it short.
    if (distanceM > 0) distancesM.set(lapId, distanceM);
  }
  if (distancesM.size < 2) return null;

  const longestM = Math.max(...distancesM.values());
  const short = [...distancesM.entries()]
    .filter(([, distanceM]) => distanceM < longestM * (1 - tolerance))
    .map(([lapId, distanceM]) => ({ lapId, distanceM }))
    .sort((a, b) => a.distanceM - b.distanceM);

  return short.length > 0 ? { longestM, short, distancesM } : null;
}

/**
 * Laps that covered materially less ground than the longest measured lap beside
 * them — the ones whose time is not a time round this track.
 *
 * The reference is the longest lap present rather than a figure per circuit, so
 * it needs no table to maintain and rights itself: a session of nothing but
 * joker laps marks none of them, because there is nothing there to call them
 * short against, and the first full lap recorded settles the rest.
 *
 * Laps the recorder could not measure carry no distance and are never marked.
 */
export function findShortLaps<
  T extends { id: string; lap_distance_m?: number | null },
>(laps: T[], tolerance = LAP_ROUTE_TOLERANCE): Set<string> {
  const measured: { id: string; distanceM: number }[] = [];
  for (const lap of laps) {
    const distanceM = lap.lap_distance_m;
    if (typeof distanceM === "number" && distanceM > 0) {
      measured.push({ id: lap.id, distanceM });
    }
  }
  if (measured.length < 2) return new Set();

  const longestM = Math.max(...measured.map((lap) => lap.distanceM));
  return new Set(
    measured
      .filter((lap) => lap.distanceM < longestM * (1 - tolerance))
      .map((lap) => lap.id),
  );
}

/** Distance for a warning line — "4.6 km", "25.4 km". */
export function formatDistanceKm(metres: number): string {
  return `${(metres / 1000).toFixed(1)} km`;
}
