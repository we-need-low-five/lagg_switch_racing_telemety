import type { DistanceSample } from "../types";
import type { TrackLayout } from "./trackLayout";

export type SectorId = "full" | "s1" | "s2" | "s3";

export interface SectorRange {
  id: SectorId;
  label: string;
  start_pct: number;
  end_pct: number;
}

export interface SectorRangesResult {
  sectors: Record<"s1" | "s2" | "s3", SectorRange>;
  approximate: boolean;
}

const FALLBACK_SPLITS: [number, number] = [33.33, 66.67];

/** ACC timing-sector boundaries as % along lap (s2 start, s3 start). */
export const ACC_SECTOR_SPLITS: Record<string, [number, number]> = {
  monza: [33.8, 67.2],
  spa: [32.5, 65.8],
  brands_hatch: [35.2, 68.5],
  silverstone: [34.1, 67.0],
  barcelona: [33.5, 66.8],
  hungaroring: [34.8, 68.2],
  zandvoort: [33.2, 66.5],
  cota: [34.5, 67.5],
  indianapolis: [35.0, 68.0],
  suzuka: [33.0, 66.2],
  nurburgring: [34.2, 67.1],
  misano: [34.6, 67.4],
  imola: [33.7, 66.9],
  kyalami: [34.0, 67.2],
  mount_panorama: [35.5, 69.0],
  laguna_seca: [33.4, 66.6],
  watkins_glen: [34.3, 67.3],
  donington: [34.8, 68.1],
  oulton_park: [35.1, 68.4],
  snetterton: [34.5, 67.6],
  paul_ricard: [33.9, 67.0],
  zolder: [34.2, 67.5],
};

export function resolveSectorSplits(layout: TrackLayout | null): {
  splits: [number, number];
  approximate: boolean;
} {
  if (layout?.sector_splits) {
    const [a, b] = layout.sector_splits;
    if (a > 0 && b > a && b < 100) {
      return { splits: layout.sector_splits, approximate: false };
    }
  }
  if (layout?.id && ACC_SECTOR_SPLITS[layout.id]) {
    return { splits: ACC_SECTOR_SPLITS[layout.id], approximate: false };
  }
  return { splits: FALLBACK_SPLITS, approximate: true };
}

export function getSectorRanges(layout: TrackLayout | null): SectorRangesResult {
  const { splits, approximate } = resolveSectorSplits(layout);
  const [s2Start, s3Start] = splits;
  return {
    approximate,
    sectors: {
      s1: { id: "s1", label: "S1", start_pct: 0, end_pct: s2Start },
      s2: { id: "s2", label: "S2", start_pct: s2Start, end_pct: s3Start },
      s3: { id: "s3", label: "S3", start_pct: s3Start, end_pct: 100 },
    },
  };
}

export function sectorRangeLabel(range: SectorRange): string {
  return `${range.label} · ${range.start_pct.toFixed(0)}–${range.end_pct.toFixed(0)}% lap`;
}

export function filterSamplesToSector(
  samples: DistanceSample[],
  range: SectorRange,
): DistanceSample[] {
  const span = range.end_pct - range.start_pct || 1;
  return samples
    .filter(
      (s) =>
        s.distance_pct >= range.start_pct - 0.05 &&
        s.distance_pct <= range.end_pct + 0.05,
    )
    .map((s) => ({
      ...s,
      distance_pct: ((s.distance_pct - range.start_pct) / span) * 100,
    }));
}

export function getActiveSectorRange(
  sectorTab: SectorId,
  ranges: SectorRangesResult,
): SectorRange | null {
  if (sectorTab === "full") return null;
  return ranges.sectors[sectorTab];
}

export function mapCursorToSectorLocal(
  lapPct: number | null,
  sectorTab: SectorId,
  ranges: SectorRangesResult,
): number | null {
  if (lapPct == null) return null;
  if (sectorTab === "full") return lapPct;
  const range = ranges.sectors[sectorTab];
  const span = range.end_pct - range.start_pct || 1;
  if (lapPct < range.start_pct - 0.05 || lapPct > range.end_pct + 0.05) return null;
  return ((lapPct - range.start_pct) / span) * 100;
}

export function mapSectorLocalToLapPct(
  localPct: number | null,
  sectorTab: SectorId,
  ranges: SectorRangesResult,
): number | null {
  if (localPct == null) return null;
  if (sectorTab === "full") return localPct;
  const range = ranges.sectors[sectorTab];
  const span = range.end_pct - range.start_pct || 1;
  return range.start_pct + (localPct / 100) * span;
}
