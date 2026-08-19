import type { DistanceSample, DistanceSampleChannel } from "../types";

export function lapHasChannel(
  samples: DistanceSample[],
  key: DistanceSampleChannel,
): boolean {
  return samples.some((s) => {
    const value = s[key];
    return value != null && typeof value === "number" && Number.isFinite(value);
  });
}

export function buildFuelUsedSamples(samples: DistanceSample[]): DistanceSample[] {
  const start = samples.find((s) => s.fuel != null && Number.isFinite(s.fuel))?.fuel;
  if (start == null) return [];
  return samples.map((s) => ({
    ...s,
    fuel:
      s.fuel != null && Number.isFinite(s.fuel)
        ? Math.max(0, start - s.fuel)
        : null,
  }));
}

export const TYRE_TEMP_CHANNELS = [
  "tyre_temp_fl",
  "tyre_temp_fr",
  "tyre_temp_rl",
  "tyre_temp_rr",
] as const satisfies readonly DistanceSampleChannel[];

export const TYRE_PRESS_CHANNELS = [
  "tyre_press_fl",
  "tyre_press_fr",
  "tyre_press_rl",
  "tyre_press_rr",
] as const satisfies readonly DistanceSampleChannel[];

export const TYRE_CORNER_LABELS: Record<(typeof TYRE_TEMP_CHANNELS)[number], string> = {
  tyre_temp_fl: "FL",
  tyre_temp_fr: "FR",
  tyre_temp_rl: "RL",
  tyre_temp_rr: "RR",
};

export const TYRE_PRESS_LABELS: Record<(typeof TYRE_PRESS_CHANNELS)[number], string> = {
  tyre_press_fl: "FL",
  tyre_press_fr: "FR",
  tyre_press_rl: "RL",
  tyre_press_rr: "RR",
};
