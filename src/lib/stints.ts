export function lapStint(lap: { stint?: number | null }): number {
  return lap.stint && lap.stint > 0 ? lap.stint : 1;
}

export function sessionHasMultipleStints(
  laps: { stint?: number | null }[],
): boolean {
  const seen = new Set(laps.map(lapStint));
  return seen.size > 1;
}

export type StintTableRow<T> =
  | { kind: "separator"; stint: number }
  | { kind: "lap"; lap: T };

export function lapsWithStintSeparators<T extends { stint?: number | null }>(
  laps: T[],
): StintTableRow<T>[] {
  if (!sessionHasMultipleStints(laps)) {
    return laps.map((lap) => ({ kind: "lap" as const, lap }));
  }
  const rows: StintTableRow<T>[] = [];
  let prev: number | null = null;
  for (const lap of laps) {
    const stint = lapStint(lap);
    if (prev !== stint) {
      rows.push({ kind: "separator", stint });
      prev = stint;
    }
    rows.push({ kind: "lap", lap });
  }
  return rows;
}
