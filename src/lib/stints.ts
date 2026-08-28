export function lapStint(lap: { stint?: number | null }): number {
  return lap.stint && lap.stint > 0 ? lap.stint : 1;
}

export function sessionHasMultipleStints(
  laps: { stint?: number | null }[],
): boolean {
  const seen = new Set(laps.map(lapStint));
  return seen.size > 1;
}

/** Compact human label for a stint break length, e.g. "45s", "12 min", "1 h 4 min". */
export function formatStintBreak(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  const mins = Math.round(s / 60);
  if (mins < 60) return `${mins} min`;
  const h = Math.floor(mins / 60);
  const rem = mins % 60;
  return rem === 0 ? `${h} h` : `${h} h ${rem} min`;
}

export type StintTableRow<T> =
  | { kind: "separator"; stint: number; breakS?: number }
  | { kind: "lap"; lap: T };

export function lapsWithStintSeparators<
  T extends { stint?: number | null; stint_break_s?: number | null },
>(laps: T[]): StintTableRow<T>[] {
  if (!sessionHasMultipleStints(laps)) {
    return laps.map((lap) => ({ kind: "lap" as const, lap }));
  }
  const rows: StintTableRow<T>[] = [];
  let prev: number | null = null;
  for (const lap of laps) {
    const stint = lapStint(lap);
    if (prev !== stint) {
      rows.push({
        kind: "separator",
        stint,
        breakS: lap.stint_break_s ?? undefined,
      });
      prev = stint;
    }
    rows.push({ kind: "lap", lap });
  }
  return rows;
}
