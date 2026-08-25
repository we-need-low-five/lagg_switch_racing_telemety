import { useMemo } from "react";
import type { GameId, LapRecord } from "../types";
import { formatLapTime } from "../types";
import {
  displaySectorTimes,
  MAX_COMPARE_LAPS,
  type CompareLapMeta,
} from "../lib/compareLaps";
import {
  formatFuelLiters,
  fuelUnitLabel,
  usePreferences,
} from "../lib/preferences";
import { computeSessionLapStats } from "../lib/sessionLapStats";

interface LapPanelProps {
  laps: LapRecord[];
  draftIds: string[];
  referenceId: string | null;
  externalMetas: CompareLapMeta[];
  canAddExternal: boolean;
  game?: GameId | null;
  onToggleLap: (id: string) => void;
  onSetReference: (id: string) => void;
  onPinLap: (lapId: string, pinned: boolean) => void;
  onLapsRefresh: () => void;
  onAddExternal: () => void;
  onCompare: () => void;
}

function formatSectorMs(ms: number | null | undefined): string {
  if (ms == null) return "—";
  return (ms / 1000).toFixed(3);
}

function formatDeltaMs(deltaMs: number | null): string {
  if (deltaMs == null) return "—";
  const sign = deltaMs > 0 ? "+" : "";
  return `${sign}${(deltaMs / 1000).toFixed(3)}`;
}

function deltaClassName(deltaMs: number | null): string | undefined {
  if (deltaMs == null) return undefined;
  if (deltaMs > 0) return "delta-negative";
  if (deltaMs < 0) return "delta-positive";
  return undefined;
}

function isBestTime(
  value: number | null | undefined,
  best: number | null | undefined,
): boolean {
  return value != null && best != null && value === best;
}

export function LapPanel({
  laps,
  draftIds,
  referenceId,
  externalMetas,
  canAddExternal,
  game = null,
  onToggleLap,
  onSetReference,
  onPinLap,
  onLapsRefresh,
  onAddExternal,
  onCompare,
}: LapPanelProps) {
  const [prefs] = usePreferences();
  const draftExternal = externalMetas.filter((m) => draftIds.includes(m.lapId));
  const fuelLabel = fuelUnitLabel(prefs.fuelUnit);
  const lapStats = useMemo(
    () => computeSessionLapStats(laps, game ?? undefined),
    [laps, game],
  );

  return (
    <div className="lap-panel expanded">
      <div className="lap-panel-header-row">
        <div className="lap-panel-header">
          <span>
            {draftIds.length} lap{draftIds.length !== 1 ? "s" : ""} selected
          </span>
        </div>
        <button
          type="button"
          className="lap-panel-add secondary"
          title="Add lap from another session"
          disabled={!canAddExternal}
          onClick={onAddExternal}
        >
          +
        </button>
      </div>

      <div className="lap-panel-expanded">
        <p className="muted small">Select up to {MAX_COMPARE_LAPS} laps</p>
        <div className="lap-list-scroll">
          <table className="lap-select-table">
            <thead>
              <tr>
                <th className="lap-select-check" />
                <th>Lap</th>
                <th>Time</th>
                <th>S1</th>
                <th>S2</th>
                <th>S3</th>
                <th>Δ Best</th>
                <th>Fuel ({fuelLabel})</th>
                <th className="lap-select-actions" />
              </tr>
            </thead>
            <tbody>
              {laps.map((lap) => {
                const sectors = displaySectorTimes(
                  lap.sectors,
                  lap.lap_time_ms,
                  game ?? undefined,
                );
                const checked = draftIds.includes(lap.id);
                const bestLap = lapStats.bestLap;
                const deltaMs =
                  bestLap && lap.id !== bestLap.id
                    ? lap.lap_time_ms - bestLap.lap_time_ms
                    : lap.is_best || bestLap?.id === lap.id
                      ? 0
                      : null;
                return (
                  <tr
                    key={lap.id}
                    className={lap.valid ? undefined : "invalid-lap"}
                    onClick={() => onToggleLap(lap.id)}
                  >
                    <td
                      className="lap-select-check"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => onToggleLap(lap.id)}
                        aria-label={`Select lap ${lap.lap_number}`}
                      />
                    </td>
                    <td>
                      <span className="lap-select-lap">
                        {lap.lap_number}
                        {lap.is_best && (
                          <span className="tag best review-inline-tag">B</span>
                        )}
                        {lap.is_pinned && (
                          <span className="tag pinned review-inline-tag">★</span>
                        )}
                        {referenceId === lap.id && (
                          <span className="tag review-inline-tag">Ref</span>
                        )}
                        {!lap.valid && (
                          <span className="tag invalid review-inline-tag">!</span>
                        )}
                      </span>
                    </td>
                    <td
                      className={
                        lapStats.bestLap?.id === lap.id ? "time-best" : undefined
                      }
                    >
                      {formatLapTime(lap.lap_time_ms)}
                    </td>
                    <td
                      className={
                        isBestTime(sectors.s1_ms, lapStats.bestS1Ms)
                          ? "time-best"
                          : undefined
                      }
                    >
                      {formatSectorMs(sectors.s1_ms)}
                    </td>
                    <td
                      className={
                        isBestTime(sectors.s2_ms, lapStats.bestS2Ms)
                          ? "time-best"
                          : undefined
                      }
                    >
                      {formatSectorMs(sectors.s2_ms)}
                    </td>
                    <td
                      className={
                        isBestTime(sectors.s3_ms, lapStats.bestS3Ms)
                          ? "time-best"
                          : undefined
                      }
                    >
                      {formatSectorMs(sectors.s3_ms)}
                    </td>
                    <td className={deltaClassName(deltaMs)}>
                      {formatDeltaMs(deltaMs)}
                    </td>
                    <td>{formatFuelLiters(lap.fuel_used_l, prefs.fuelUnit)}</td>
                    <td
                      className="lap-select-actions"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <div className="lap-actions">
                        <button
                          type="button"
                          className="secondary"
                          onClick={() => onSetReference(lap.id)}
                        >
                          Ref
                        </button>
                        <button
                          type="button"
                          className="secondary"
                          onClick={async () => {
                            await onPinLap(lap.id, !lap.is_pinned);
                            onLapsRefresh();
                          }}
                        >
                          {lap.is_pinned ? "Unpin" : "Pin"}
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}

              {draftExternal.map((meta) => {
                const sectors = displaySectorTimes(
                  meta.sectors,
                  meta.lapTimeMs,
                  game ?? undefined,
                );
                const checked = draftIds.includes(meta.lapId);
                const bestLap = lapStats.bestLap;
                const deltaMs =
                  bestLap != null
                    ? meta.lapTimeMs - bestLap.lap_time_ms
                    : null;
                return (
                  <tr
                    key={meta.lapId}
                    onClick={() => onToggleLap(meta.lapId)}
                  >
                    <td
                      className="lap-select-check"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => onToggleLap(meta.lapId)}
                        aria-label={`Select ${meta.playerName}`}
                      />
                    </td>
                    <td>
                      <span className="lap-select-lap">
                        {meta.playerName}
                        <span className="tag review-inline-tag">External</span>
                        {referenceId === meta.lapId && (
                          <span className="tag review-inline-tag">Ref</span>
                        )}
                      </span>
                    </td>
                    <td>{formatLapTime(meta.lapTimeMs)}</td>
                    <td>{formatSectorMs(sectors.s1_ms)}</td>
                    <td>{formatSectorMs(sectors.s2_ms)}</td>
                    <td>{formatSectorMs(sectors.s3_ms)}</td>
                    <td className={deltaClassName(deltaMs)}>
                      {formatDeltaMs(deltaMs)}
                    </td>
                    <td>—</td>
                    <td
                      className="lap-select-actions"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <div className="lap-actions">
                        <button
                          type="button"
                          className="secondary"
                          onClick={() => onSetReference(meta.lapId)}
                        >
                          Ref
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
        <div className="lap-panel-footer">
          <button
            type="button"
            className="primary lap-panel-compare"
            disabled={draftIds.length === 0}
            onClick={onCompare}
          >
            Compare
          </button>
        </div>
      </div>
    </div>
  );
}
