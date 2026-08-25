import {
  BACKGROUND_PRESETS,
  type AppearancePrefs,
  getPreferences,
} from "./preferences";
import paletteRecipe from "./palette-recipe.json";

export type Hsl = { h: number; s: number; l: number };

type SlotDelta = {
  dH: number;
  dS: number;
  dL: number;
};

type AdaptedSlot = {
  hex: string;
  clamped: boolean;
};

type TransferMode = "all" | "hue-only";

const RECIPE = {
  colors: paletteRecipe.colors,
  seedIndex: paletteRecipe.seedIndex,
  mode: paletteRecipe.mode as TransferMode,
  flags: paletteRecipe.flags,
};

const HEX6 = /^#([0-9a-fA-F]{6})$/;
const HEX3 = /^#([0-9a-fA-F]{3})$/;

function round1(n: number): number {
  return Math.round(n * 10) / 10;
}

function clamp(n: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, n));
}

function wrapHue(h: number): number {
  return ((h % 360) + 360) % 360;
}

/** Signed shortest-arc hue delta in (−180, 180]. */
function signedHueDelta(from: number, to: number): number {
  let d = ((to - from) % 360 + 360) % 360;
  if (d > 180) d -= 360;
  return round1(d);
}

export function parseHex(input: string): string | null {
  const trimmed = input.trim();
  const withHash = trimmed.startsWith("#") ? trimmed : `#${trimmed}`;
  const short = HEX3.exec(withHash);
  if (short) {
    const [r, g, b] = short[1];
    return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
  }
  const full = HEX6.exec(withHash);
  if (full) return `#${full[1]}`.toLowerCase();
  return null;
}

function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
  const normalized = parseHex(hex);
  if (!normalized) return null;
  const n = parseInt(normalized.slice(1), 16);
  return {
    r: (n >> 16) & 255,
    g: (n >> 8) & 255,
    b: n & 255,
  };
}

function rgbToHex(r: number, g: number, b: number): string {
  const to = (c: number) =>
    clamp(Math.round(c), 0, 255).toString(16).padStart(2, "0");
  return `#${to(r)}${to(g)}${to(b)}`;
}

export function hexToHsl(hex: string): Hsl | null {
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

function hueToRgb(p: number, q: number, t: number): number {
  let x = t;
  if (x < 0) x += 1;
  if (x > 1) x -= 1;
  if (x < 1 / 6) return p + (q - p) * 6 * x;
  if (x < 1 / 2) return q;
  if (x < 2 / 3) return p + (q - p) * (2 / 3 - x) * 6;
  return p;
}

export function hslToHex(hsl: Hsl): string {
  const h = wrapHue(hsl.h) / 360;
  const s = clamp(hsl.s, 0, 100) / 100;
  const l = clamp(hsl.l, 0, 100) / 100;
  let r: number;
  let g: number;
  let b: number;
  if (s === 0) {
    r = g = b = l;
  } else {
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    r = hueToRgb(p, q, h + 1 / 3);
    g = hueToRgb(p, q, h);
    b = hueToRgb(p, q, h - 1 / 3);
  }
  return rgbToHex(r * 255, g * 255, b * 255);
}

function extractRecipe(colors: string[], seedIndex: number): SlotDelta[] {
  const seed = hexToHsl(colors[seedIndex]);
  if (!seed) return colors.map(() => ({ dH: 0, dS: 0, dL: 0 }));
  return colors.map((hex, i) => {
    if (i === seedIndex) return { dH: 0, dS: 0, dL: 0 };
    const hsl = hexToHsl(hex);
    if (!hsl) return { dH: 0, dS: 0, dL: 0 };
    return {
      dH: signedHueDelta(seed.h, hsl.h),
      dS: round1(hsl.s - seed.s),
      dL: round1(hsl.l - seed.l),
    };
  });
}

function applyDeltaToHex(seedHex: string, delta: SlotDelta): AdaptedSlot {
  const seed = hexToHsl(seedHex);
  if (!seed) return { hex: seedHex, clamped: false };
  const sRaw = seed.s + delta.dS;
  const lRaw = seed.l + delta.dL;
  const s = clamp(sRaw, 0, 100);
  const l = clamp(lRaw, 0, 100);
  return {
    hex: hslToHex({ h: wrapHue(seed.h + delta.dH), s, l }),
    clamped: s !== sRaw || l !== lRaw,
  };
}

function adaptPalette(
  colors: string[],
  seedIndex: number,
  newSeed: string,
  mode: TransferMode,
  recipe: SlotDelta[],
): AdaptedSlot[] {
  return colors.map((hex, i) => {
    if (i === seedIndex) return { hex: newSeed, clamped: false };
    const delta = recipe[i] ?? { dH: 0, dS: 0, dL: 0 };
    if (mode === "hue-only") {
      const orig = hexToHsl(hex);
      const next = hexToHsl(newSeed);
      if (!orig || !next) return { hex, clamped: false };
      return {
        hex: hslToHex({ h: wrapHue(next.h + delta.dH), s: orig.s, l: orig.l }),
        clamped: false,
      };
    }
    return applyDeltaToHex(newSeed, delta);
  });
}

function hasFlag(flags: string[][], name: string): boolean {
  return flags.some((list) => list.includes(name));
}

function slotHex(
  adapted: AdaptedSlot[],
  flags: string[][],
  names: string[],
  fallbackIndex: number,
): string {
  for (const name of names) {
    const index = flags.findIndex((list) => list.includes(name));
    if (index >= 0 && adapted[index]) return adapted[index].hex;
  }
  const i = Math.min(Math.max(fallbackIndex, 0), adapted.length - 1);
  return adapted[i].hex;
}

function fillIsDark(hex: string): boolean {
  const rgb = hexToRgb(hex);
  if (!rgb) return true;
  const y = (0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b) / 255;
  return y <= 0.55;
}

/** Prefer the raw seed when it contrasts with the surface; else keep its hue at a readable lightness. */
function navIconColorFromSeed(seedHex: string, surfaceHex: string): string {
  const seed = hexToHsl(seedHex);
  const surface = hexToHsl(surfaceHex);
  if (!seed) return seedHex;
  const surfaceL = surface?.l ?? 20;
  if (Math.abs(seed.l - surfaceL) >= 28) {
    return parseHex(seedHex) ?? seedHex;
  }
  return hslToHex({
    h: seed.h,
    s: Math.max(seed.s, 42),
    l: surfaceL < 50 ? 72 : 36,
  });
}

/** hsl(h s% l%) — modern space-separated syntax for alpha mixes */
export function themeHsl(h: number, s: number, l: number, alpha?: number): string {
  const base = `hsl(${round1(wrapHue(h))} ${round1(clamp(s, 0, 100))}% ${round1(clamp(l, 0, 100))}%`;
  return alpha == null ? `${base})` : `${base} / ${alpha})`;
}

function layered(
  hex: string,
  ds: number,
  dl: number,
  towardLight: boolean,
): string {
  const hsl = hexToHsl(hex);
  if (!hsl) return hex;
  const dir = towardLight ? 1 : -1;
  return themeHsl(hsl.h, hsl.s + ds, hsl.l + dir * dl);
}

function tintedText(seedHex: string, s: number, l: number): string {
  const hsl = hexToHsl(seedHex);
  return themeHsl(hsl?.h ?? 275, s, l);
}

export interface ThemePalette {
  h: number;
  scheme: "dark" | "light";
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

const RECIPE_DELTAS = extractRecipe(RECIPE.colors, RECIPE.seedIndex);

/**
 * Retint the stored palette recipe onto `seedHex`.
 * Mode `all` applies ΔH/ΔS/ΔL from the recipe seed; `hue-only` keeps each
 * slot's original S/L and only transfers hue. Flagged slots map onto UI
 * tokens; remaining chrome (hover, borders, text) is derived from those fills.
 */
export function buildThemePalette(seedHex: string): ThemePalette {
  const seed =
    parseHex(seedHex) ?? parseHex(RECIPE.colors[RECIPE.seedIndex]) ?? "#221c26";
  const adapted = adaptPalette(
    RECIPE.colors,
    RECIPE.seedIndex,
    seed,
    RECIPE.mode,
    RECIPE_DELTAS,
  );
  const last = adapted.length - 1;
  const { flags } = RECIPE;

  const bgBase = slotHex(adapted, flags, ["--background"], RECIPE.seedIndex);
  const bgSurface = slotHex(adapted, flags, ["--card"], Math.min(1, last));
  const bgControl = slotHex(
    adapted,
    flags,
    ["--primary", "--secondary", "--accent"],
    Math.min(2, last),
  );
  const accent = slotHex(
    adapted,
    flags,
    ["--accent", "--primary", "--secondary"],
    Math.min(2, last),
  );

  const dark = fillIsDark(bgBase);
  const controlDark = fillIsDark(bgControl);
  const seedHsl = hexToHsl(bgBase);
  const accentHsl = hexToHsl(accent);
  const towardLight = dark;

  const textPrimary = hasFlag(flags, "--foreground")
    ? slotHex(adapted, flags, ["--foreground"], RECIPE.seedIndex)
    : tintedText(bgBase, 20, dark ? 92 : 12);
  const textMuted = hasFlag(flags, "--muted")
    ? slotHex(adapted, flags, ["--muted"], RECIPE.seedIndex)
    : tintedText(bgBase, 12, dark ? 62 : 42);
  const borderColor = hasFlag(flags, "--border")
    ? slotHex(adapted, flags, ["--border"], Math.min(1, last))
    : layered(bgSurface, 0, 10, towardLight);
  const linkColor = hasFlag(flags, "--link")
    ? slotHex(adapted, flags, ["--link"], Math.min(2, last))
    : themeHsl(
        accentHsl?.h ?? seedHsl?.h ?? 275,
        Math.max(accentHsl?.s ?? 15, 50),
        dark ? 68 : 38,
      );

  return {
    h: round1(wrapHue(seedHsl?.h ?? 275)),
    scheme: dark ? "dark" : "light",
    bgBase,
    bgSurface,
    bgSurfaceElevated: layered(bgSurface, 2, 2, towardLight),
    bgControl,
    accent,
    accentHover: layered(accent, 3, 7, towardLight),
    accentActive: layered(accent, 20, 15, towardLight),
    borderColor,
    borderControl: layered(bgControl, 0, 13, towardLight),
    textPrimary,
    textMuted,
    textSecondary: tintedText(bgBase, 15, dark ? 78 : 28),
    textOnControl: tintedText(bgControl, 25, controlDark ? 95 : 12),
    linkColor,
    overlayBackdrop: `color-mix(in srgb, ${bgBase} 72%, transparent)`,
    chartGrid: layered(bgSurface, 0, 6, towardLight),
    chartAxis: textMuted,
    scrollbarThumb: layered(bgControl, 0, 10, towardLight),
    scrollbarTrack: layered(bgSurface, 0, -2, towardLight),
  };
}

export function resolveBackgroundBase(
  appearance: Pick<AppearancePrefs, "backgroundPreset" | "backgroundCustom">,
): string {
  if (appearance.backgroundCustom) {
    return appearance.backgroundCustom;
  }
  return BACKGROUND_PRESETS[appearance.backgroundPreset] ?? BACKGROUND_PRESETS.forest;
}

/** Chart-friendly hex of the theme accent (same hue as `--link-color`). */
export function lapAccentHex(seedHex: string): string {
  const palette = buildThemePalette(seedHex);
  const accentHsl = hexToHsl(palette.accent);
  return hslToHex({
    h: accentHsl?.h ?? palette.h,
    s: Math.max(accentHsl?.s ?? 15, 50),
    l: palette.scheme === "dark" ? 68 : 38,
  });
}

export function hueFromAppearance(appearance: AppearancePrefs): number {
  const base = resolveBackgroundBase(appearance);
  const hsl = hexToHsl(base);
  return hsl?.h ?? 275;
}

export function applyTheme(appearance?: AppearancePrefs): void {
  const prefs = appearance ?? getPreferences().appearance;
  const seedHex = resolveBackgroundBase(prefs);
  const palette = buildThemePalette(seedHex);
  const root = document.documentElement;
  const navIconColor = navIconColorFromSeed(seedHex, palette.bgSurface);

  root.style.setProperty("--theme-h", String(palette.h));
  root.style.setProperty("--theme-seed", parseHex(seedHex) ?? seedHex);
  root.style.setProperty("--nav-icon-color", navIconColor);
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

  root.dataset.theme = palette.scheme;
  root.style.colorScheme = palette.scheme;

  document.body.style.background = palette.bgBase;
}
