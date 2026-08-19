import {
  BACKGROUND_PRESETS,
  type AppearancePrefs,
  getPreferences,
} from "./preferences";

function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
  const cleaned = hex.replace("#", "");
  if (cleaned.length !== 6) return null;
  const n = parseInt(cleaned, 16);
  return {
    r: (n >> 16) & 255,
    g: (n >> 8) & 255,
    b: n & 255,
  };
}

function hexToHsl(hex: string): { h: number; s: number; l: number } | null {
  const rgb = hexToRgb(hex);
  if (!rgb) return null;

  const r = rgb.r / 255;
  const g = rgb.g / 255;
  const b = rgb.b / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0;
  let s = 0;
  const l = (max + min) / 2;

  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case r:
        h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
        break;
      case g:
        h = ((b - r) / d + 2) / 6;
        break;
      default:
        h = ((r - g) / d + 4) / 6;
        break;
    }
  }

  return { h: h * 360, s: s * 100, l: l * 100 };
}

function wrapHue(h: number): number {
  return ((h % 360) + 360) % 360;
}

/** hsl(h, s%, l%) — modern space-separated syntax for alpha mixes */
export function themeHsl(h: number, s: number, l: number, alpha?: number): string {
  const base = `hsl(${wrapHue(h)} ${s}% ${l}%`;
  return alpha == null ? `${base})` : `${base} / ${alpha})`;
}

export interface ThemePalette {
  h: number;
  bgBase: string;
  bgSurface: string;
  bgSurfaceElevated: string;
  bgControl: string;
  accent: string;
  accentHover: string;
  accentActive: string;
  borderColor: string;
  borderControl: string;
  textPrimary: string;
  textMuted: string;
  textSecondary: string;
  textOnControl: string;
  linkColor: string;
  overlayBackdrop: string;
  chartGrid: string;
  chartAxis: string;
  scrollbarThumb: string;
  scrollbarTrack: string;
}

const PANEL_HUE_FACTOR = 1.136;
const CONTROL_HUE_FACTOR = 1.207;

function panelHue(h: number): number {
  return wrapHue(h * PANEL_HUE_FACTOR);
}

function controlHue(h: number): number {
  return wrapHue(h * CONTROL_HUE_FACTOR);
}

/** Palette: background h,39,7 · panels h×1.136,9,12 · controls h×1.207,15,15 */
export function buildThemePalette(h: number): ThemePalette {
  const panelH = panelHue(h);
  const controlH = controlHue(h);
  const bgBase = themeHsl(h, 39, 7);
  const bgSurface = themeHsl(panelH, 9, 12);
  const bgControl = themeHsl(controlH, 15, 15);

  return {
    h: wrapHue(h),
    bgBase,
    bgSurface,
    bgSurfaceElevated: themeHsl(panelH, 11, 14),
    bgControl,
    accent: bgControl,
    accentHover: themeHsl(controlH, 18, 22),
    accentActive: themeHsl(controlH, 35, 30),
    borderColor: themeHsl(panelH, 9, 22),
    borderControl: themeHsl(controlH, 15, 28),
    textPrimary: themeHsl(h, 20, 92),
    textMuted: themeHsl(h, 12, 62),
    textSecondary: themeHsl(h, 15, 78),
    textOnControl: themeHsl(h, 25, 95),
    linkColor: themeHsl(controlH, 50, 68),
    overlayBackdrop: themeHsl(h, 39, 7, 0.72),
    chartGrid: themeHsl(panelH, 9, 18),
    chartAxis: themeHsl(h, 12, 62),
    scrollbarThumb: themeHsl(controlH, 15, 25),
    scrollbarTrack: themeHsl(panelH, 9, 10),
  };
}

export function resolveBackgroundBase(appearance: AppearancePrefs): string {
  if (appearance.backgroundCustom) {
    return appearance.backgroundCustom;
  }
  return BACKGROUND_PRESETS[appearance.backgroundPreset] ?? BACKGROUND_PRESETS.slate;
}

export function hueFromAppearance(appearance: AppearancePrefs): number {
  const base = resolveBackgroundBase(appearance);
  const hsl = hexToHsl(base);
  return hsl?.h ?? 220;
}

export function applyTheme(appearance?: AppearancePrefs): void {
  const prefs = appearance ?? getPreferences().appearance;
  const palette = buildThemePalette(hueFromAppearance(prefs));
  const root = document.documentElement;

  root.style.setProperty("--theme-h", String(palette.h));
  root.style.setProperty("--bg-base", palette.bgBase);
  root.style.setProperty("--bg-surface", palette.bgSurface);
  root.style.setProperty("--bg-surface-elevated", palette.bgSurfaceElevated);
  root.style.setProperty("--bg-control", palette.bgControl);
  root.style.setProperty("--accent", palette.accent);
  root.style.setProperty("--accent-hover", palette.accentHover);
  root.style.setProperty("--accent-active", palette.accentActive);
  root.style.setProperty("--border-color", palette.borderColor);
  root.style.setProperty("--border-control", palette.borderControl);
  root.style.setProperty("--text-primary", palette.textPrimary);
  root.style.setProperty("--text-muted", palette.textMuted);
  root.style.setProperty("--text-secondary", palette.textSecondary);
  root.style.setProperty("--text-on-control", palette.textOnControl);
  root.style.setProperty("--link-color", palette.linkColor);
  root.style.setProperty("--overlay-backdrop", palette.overlayBackdrop);
  root.style.setProperty("--chart-grid", palette.chartGrid);
  root.style.setProperty("--chart-axis", palette.chartAxis);
  root.style.setProperty("--scrollbar-thumb", palette.scrollbarThumb);
  root.style.setProperty("--scrollbar-track", palette.scrollbarTrack);
  root.style.setProperty("--track-map-bg", palette.bgSurfaceElevated);
  root.style.setProperty("--track-map-outline", palette.borderColor);
  root.style.setProperty("--track-map-outline-inner", palette.textMuted);
  root.style.setProperty("--track-map-cursor-fill", palette.textOnControl);
  root.style.setProperty("--track-map-cursor-stroke", palette.linkColor);

  root.dataset.theme = "dark";
  root.style.colorScheme = "dark";

  document.body.style.background = palette.bgBase;
}
