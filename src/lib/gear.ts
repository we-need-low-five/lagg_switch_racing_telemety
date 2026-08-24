/** Hold last engaged gear through ACC/AC N=1 (and F1 N=0) blips on shifts. */
export function holdGearThroughNeutral(gears: number[]): number[] {
  if (gears.length === 0) return [];
  const out = new Array<number>(gears.length);
  let held = Math.round(gears[0]);
  out[0] = held;
  for (let i = 1; i < gears.length; i += 1) {
    const current = Number(gears[i]);
    if (!Number.isFinite(current)) {
      out[i] = held;
      continue;
    }
    const g = Math.round(current);
    if (Math.abs(g - held) <= 1) {
      held = g;
    } else if (g <= 1 && held >= 2) {
      // keep held
    } else {
      held = g;
    }
    out[i] = held;
  }
  return out;
}
