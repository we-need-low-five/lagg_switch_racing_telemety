import { useCallback, useEffect, useState } from "react";

export interface AppearancePrefs {
  backgroundPreset: string;
  backgroundCustom: string;
  lapColors: string[];
}

export interface LayoutPrefs {
  columnSplitPct: number;
  mapLapSplitPct: number;
  chartHeights: Record<string, number>;
  chartCollapsed: Record<string, boolean>;
  mapCollapsed: boolean;
  tractionCircleCollapsed: boolean;
  lapsCollapsed: boolean;
}

export type SpeedUnit = "kmh" | "mph";
export type DeltaUnit = "s" | "ms";
export type FuelUnit = "l" | "us_gal";
export type TempUnit = "c" | "f";
export type PressureUnit = "psi" | "bar";

export interface UserPreferences {
  speedUnit: SpeedUnit;
  deltaUnit: DeltaUnit;
  fuelUnit: FuelUnit;
  tempUnit: TempUnit;
  pressureUnit: PressureUnit;
  appearance: AppearancePrefs;
  layout: LayoutPrefs;
}

const STORAGE_KEY = "simtelemetry.preferences";

export const DEFAULT_LAP_COLORS = [
  "#38bdf8",
  "#f472b6",
  "#fbbf24",
  "#4ade80",
];

const DEFAULT_APPEARANCE: AppearancePrefs = {
  backgroundPreset: "slate",
  backgroundCustom: "",
  lapColors: [...DEFAULT_LAP_COLORS],
};

export const BACKGROUND_PRESETS: Record<string, string> = {
  slate: "#020617",
  midnight: "#0a0a0f",
  charcoal: "#1a1a1a",
  ocean: "#0c1929",
  forest: "#0d1f17",
  dusk: "#1e1b4b",
  light: "#f1f5f9",
};

const DEFAULT_LAYOUT: LayoutPrefs = {
  columnSplitPct: 65,
  mapLapSplitPct: 55,
  chartHeights: {},
  chartCollapsed: {},
  mapCollapsed: false,
  tractionCircleCollapsed: false,
  lapsCollapsed: false,
};

const DEFAULTS: UserPreferences = {
  speedUnit: "kmh",
  deltaUnit: "s",
  fuelUnit: "l",
  tempUnit: "c",
  pressureUnit: "psi",
  appearance: { ...DEFAULT_APPEARANCE },
  layout: { ...DEFAULT_LAYOUT },
};

function mergeAppearance(
  parsed: Partial<AppearancePrefs> | undefined,
): AppearancePrefs {
  if (!parsed) return { ...DEFAULT_APPEARANCE, lapColors: [...DEFAULT_LAP_COLORS] };
  return {
    backgroundPreset:
      parsed.backgroundPreset && BACKGROUND_PRESETS[parsed.backgroundPreset]
        ? parsed.backgroundPreset
        : DEFAULT_APPEARANCE.backgroundPreset,
    backgroundCustom: parsed.backgroundCustom ?? "",
    lapColors:
      parsed.lapColors?.length === DEFAULT_LAP_COLORS.length
        ? [...parsed.lapColors]
        : [...DEFAULT_LAP_COLORS],
  };
}

function mergeLayout(parsed: Partial<LayoutPrefs> | undefined): LayoutPrefs {
  if (!parsed) return { ...DEFAULT_LAYOUT, chartHeights: {}, chartCollapsed: {} };
  return {
    columnSplitPct: parsed.columnSplitPct ?? DEFAULT_LAYOUT.columnSplitPct,
    mapLapSplitPct: parsed.mapLapSplitPct ?? DEFAULT_LAYOUT.mapLapSplitPct,
    chartHeights: { ...parsed.chartHeights },
    chartCollapsed: { ...parsed.chartCollapsed },
    mapCollapsed: parsed.mapCollapsed ?? false,
    tractionCircleCollapsed: parsed.tractionCircleCollapsed ?? false,
    lapsCollapsed: parsed.lapsCollapsed ?? false,
  };
}

function readPreferences(): UserPreferences {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return {
        ...DEFAULTS,
        appearance: mergeAppearance(undefined),
        layout: mergeLayout(undefined),
      };
    }
    const parsed = JSON.parse(raw) as Partial<UserPreferences>;
    return {
      speedUnit: parsed.speedUnit === "mph" ? "mph" : "kmh",
      deltaUnit: parsed.deltaUnit === "ms" ? "ms" : "s",
      fuelUnit: parsed.fuelUnit === "us_gal" ? "us_gal" : "l",
      tempUnit: parsed.tempUnit === "f" ? "f" : "c",
      pressureUnit: parsed.pressureUnit === "bar" ? "bar" : "psi",
      appearance: mergeAppearance(parsed.appearance),
      layout: mergeLayout(parsed.layout),
    };
  } catch {
    return {
      ...DEFAULTS,
      appearance: mergeAppearance(undefined),
      layout: mergeLayout(undefined),
    };
  }
}

export function getPreferences(): UserPreferences {
  return readPreferences();
}

export type PreferencesPatch = {
  speedUnit?: SpeedUnit;
  deltaUnit?: DeltaUnit;
  fuelUnit?: FuelUnit;
  tempUnit?: TempUnit;
  pressureUnit?: PressureUnit;
  appearance?: Partial<AppearancePrefs>;
  layout?: Partial<LayoutPrefs>;
};

export function setPreferences(patch: PreferencesPatch): UserPreferences {
  const current = readPreferences();
  const next: UserPreferences = {
    speedUnit: patch.speedUnit ?? current.speedUnit,
    deltaUnit: patch.deltaUnit ?? current.deltaUnit,
    fuelUnit: patch.fuelUnit ?? current.fuelUnit,
    tempUnit: patch.tempUnit ?? current.tempUnit,
    pressureUnit: patch.pressureUnit ?? current.pressureUnit,
    appearance: patch.appearance
      ? mergeAppearance({ ...current.appearance, ...patch.appearance })
      : current.appearance,
    layout: patch.layout
      ? mergeLayout({ ...current.layout, ...patch.layout })
      : current.layout,
  };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  window.dispatchEvent(new CustomEvent("simtelemetry-preferences"));
  return next;
}

export function resetAppearance(): UserPreferences {
  return setPreferences({
    appearance: {
      ...DEFAULT_APPEARANCE,
      lapColors: [...DEFAULT_LAP_COLORS],
    },
  });
}

export function getLapColor(index: number): string {
  const colors = readPreferences().appearance.lapColors;
  return colors[index % colors.length];
}

export function usePreferences(): [
  UserPreferences,
  (patch: PreferencesPatch) => void,
] {
  const [prefs, setPrefs] = useState<UserPreferences>(readPreferences);

  useEffect(() => {
    const onChange = () => setPrefs(readPreferences());
    window.addEventListener("simtelemetry-preferences", onChange);
    return () => window.removeEventListener("simtelemetry-preferences", onChange);
  }, []);

  const update = useCallback((patch: PreferencesPatch) => {
    setPrefs(setPreferences(patch));
  }, []);

  return [prefs, update];
}

export function speedMpsToDisplay(mps: number, unit: SpeedUnit): number {
  return unit === "mph" ? mps * 2.23694 : mps * 3.6;
}

export function speedUnitLabel(unit: SpeedUnit): string {
  return unit === "mph" ? "mph" : "km/h";
}

export function fuelLitersToDisplay(l: number, unit: FuelUnit): number {
  return unit === "us_gal" ? l * 0.264172 : l;
}

export function fuelUnitLabel(unit: FuelUnit): string {
  return unit === "us_gal" ? "US gal" : "L";
}

export function formatFuelLiters(l: number | null | undefined, unit: FuelUnit): string {
  if (l == null || !Number.isFinite(l)) return "—";
  const value = fuelLitersToDisplay(l, unit);
  return value.toFixed(2);
}

export function tempCToDisplay(c: number, unit: TempUnit): number {
  return unit === "f" ? c * (9 / 5) + 32 : c;
}

export function tempUnitLabel(unit: TempUnit): string {
  return unit === "f" ? "°F" : "°C";
}

export function pressurePsiToDisplay(psi: number, unit: PressureUnit): number {
  return unit === "bar" ? psi * 0.0689476 : psi;
}

export function pressureUnitLabel(unit: PressureUnit): string {
  return unit === "bar" ? "bar" : "PSI";
}

export function formatDeltaValue(seconds: number, unit: DeltaUnit): string {
  if (unit === "ms") {
    const ms = Math.round(seconds * 1000);
    const sign = ms > 0 ? "+" : "";
    return `${sign}${ms} ms`;
  }
  const sign = seconds > 0 ? "+" : "";
  return `${sign}${seconds.toFixed(3)} s`;
}

export const DEFAULT_CHART_HEIGHT = 280;

export function getChartHeight(key: string): number {
  const heights = readPreferences().layout.chartHeights;
  return heights[key] ?? DEFAULT_CHART_HEIGHT;
}

export function isChartCollapsed(key: string): boolean {
  return readPreferences().layout.chartCollapsed[key] ?? false;
}
