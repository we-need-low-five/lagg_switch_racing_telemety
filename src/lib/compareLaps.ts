import type { GameId, LapRecord, TrackLapOption } from "../types";
import { formatLapTime } from "../types";

export const MAX_COMPARE_LAPS = 4;

export type CompareMode = "session" | "global";

export interface CompareLapMeta {
  lapId: string;
  sessionId: string;
  lapNumber: number;
  lapTimeMs: number;
  valid: boolean;
  playerName: string;
  car: string;
  startedAt: string;
  isBest?: boolean;
  isPinned?: boolean;
  sectors: LapRecord["sectors"];
  sampleRateHz: number;
  isExternal: boolean;
}

export function lapRecordToMeta(
  lap: LapRecord,
  playerName: string,
  car: string,
  startedAt: string,
  isExternal = false,
): CompareLapMeta {
  return {
    lapId: lap.id,
    sessionId: lap.session_id,
    lapNumber: lap.lap_number,
    lapTimeMs: lap.lap_time_ms,
    valid: lap.valid,
    playerName,
    car,
    startedAt,
    isBest: lap.is_best,
    isPinned: lap.is_pinned,
    sectors: lap.sectors,
    sampleRateHz: lap.sample_rate_hz,
    isExternal,
  };
}

export function trackLapOptionToMeta(option: TrackLapOption): CompareLapMeta {
  return {
    lapId: option.lap_id,
    sessionId: option.session_id,
    lapNumber: option.lap_number,
    lapTimeMs: option.lap_time_ms,
    valid: option.valid,
    playerName: option.player_name,
    car: option.car,
    startedAt: option.started_at,
    sectors: option.sectors ?? {},
    sampleRateHz: 0,
    isExternal: true,
  };
}

export function formatCompareLapLabel(
  meta: CompareLapMeta,
  mode: CompareMode,
): string {
  if (mode === "global" || meta.isExternal) {
    return `${meta.playerName} · ${formatLapTime(meta.lapTimeMs)}`;
  }
  const suffix = meta.isBest ? " (best)" : meta.isPinned ? " ★" : "";
  return `Lap ${meta.lapNumber}${suffix}`;
}

export function formatCompareDeltaLabel(
  meta: CompareLapMeta,
  mode: CompareMode,
): string {
  if (mode === "global" || meta.isExternal) {
    return `Δ ${meta.playerName}`;
  }
  return `Δ Lap ${meta.lapNumber}`;
}

export function canAddLap(selectedCount: number): boolean {
  return selectedCount < MAX_COMPARE_LAPS;
}

/** Mirror backend normalize_sector_times for stored lap sector JSON. */
export function normalizeSectorTimes(
  sectors: LapRecord["sectors"],
  lapTimeMs: number,
): LapRecord["sectors"] {
  const s1 = sectors.s1_ms;
  const s2 = sectors.s2_ms;
  const s3 = sectors.s3_ms;

  if (s1 != null && s2 != null && s3 != null) {
    const sum = s1 + s2 + s3;
    if (
      lapTimeMs > 0 &&
      sum > 0 &&
      sum <= lapTimeMs + 500 &&
      sum >= lapTimeMs - 2000
    ) {
      return sectors;
    }
  }

  if (s1 != null && s2 != null && s3 == null) {
    const sumAb = s1 + s2;
    if (
      lapTimeMs > 0 &&
      sumAb < (lapTimeMs * 9) / 10 &&
      lapTimeMs > sumAb + 500
    ) {
      return {
        s1_ms: s1,
        s2_ms: s2,
        s3_ms: lapTimeMs - sumAb,
      };
    }
  }

  const looksCumulative =
    s1 != null &&
    s2 != null &&
    s2 > s1 &&
    lapTimeMs > 0 &&
    (s1 + s2 > lapTimeMs - lapTimeMs / 10 ||
      s2 > (lapTimeMs * 2) / 3);

  if (looksCumulative) {
    const s3Total = s3 ?? lapTimeMs;
    return {
      s1_ms: s1,
      s2_ms: s1 != null && s2 != null && s2 >= s1 ? s2 - s1 : null,
      s3_ms: s2 != null && s3Total >= s2 ? s3Total - s2 : null,
    };
  }

  if (s3 == null && lapTimeMs > 0) {
    const prior =
      s1 != null && s2 != null
        ? s1 + s2
        : s1 ?? 0;
    if (lapTimeMs > prior) {
      return {
        s1_ms: s1,
        s2_ms: s2,
        s3_ms: lapTimeMs - prior,
      };
    }
  }

  return sectors;
}

/** ACC/AC store cumulative splits at S1/S2 lines; derive per-sector for display. */
export function accCumulativeSplitsToSectors(
  cumS1Ms: number | null | undefined,
  cumS2Ms: number | null | undefined,
  lapTimeMs: number,
): LapRecord["sectors"] | null {
  if (lapTimeMs <= 0) return null;
  if (cumS1Ms != null && cumS2Ms != null && cumS2Ms >= cumS1Ms && lapTimeMs >= cumS2Ms) {
    return {
      s1_ms: cumS1Ms,
      s2_ms: cumS2Ms - cumS1Ms,
      s3_ms: lapTimeMs - cumS2Ms,
    };
  }
  if (cumS1Ms != null && cumS2Ms == null && lapTimeMs > cumS1Ms) {
    return {
      s1_ms: cumS1Ms,
      s2_ms: null,
      s3_ms: lapTimeMs - cumS1Ms,
    };
  }
  return null;
}

function looksLikeAccCumulativeSplits(
  sectors: LapRecord["sectors"],
  lapTimeMs: number,
): boolean {
  const { s1_ms: s1, s2_ms: s2, s3_ms: s3 } = sectors;
  if (s1 == null || s2 == null || lapTimeMs <= 0) return false;
  if (s3 != null) {
    const sum = s1 + s2 + s3;
    if (sum <= lapTimeMs + 500 && sum >= lapTimeMs - 2000) {
      return false;
    }
  }
  return s2 > s1 && s1 + s2 > (lapTimeMs * 9) / 10;
}

/** Format sectors for UI; repairs ACC/AC cumulative splits in older stored laps. */
export function displaySectorTimes(
  sectors: LapRecord["sectors"],
  lapTimeMs: number,
  game?: GameId,
): LapRecord["sectors"] {
  if (game === "acc" || game === "ac") {
    if (looksLikeAccCumulativeSplits(sectors, lapTimeMs)) {
      const repaired = accCumulativeSplitsToSectors(
        sectors.s1_ms,
        sectors.s2_ms,
        lapTimeMs,
      );
      if (repaired) return repaired;
    }
  }
  return normalizeSectorTimes(sectors, lapTimeMs);
}

export function toggleSelectedId(
  current: string[],
  lapId: string,
): string[] {
  if (current.includes(lapId)) {
    return current.filter((id) => id !== lapId);
  }
  if (current.length >= MAX_COMPARE_LAPS) return current;
  return [...current, lapId];
}

export function mergeCatalog(
  sessionMetas: CompareLapMeta[],
  externalMetas: CompareLapMeta[],
): CompareLapMeta[] {
  const map = new Map<string, CompareLapMeta>();
  for (const meta of sessionMetas) map.set(meta.lapId, meta);
  for (const meta of externalMetas) map.set(meta.lapId, meta);
  return Array.from(map.values());
}
