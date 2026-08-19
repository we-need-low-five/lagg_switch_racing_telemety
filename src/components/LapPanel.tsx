import { useState } from "react";
import type { LapRecord } from "../types";
import { formatLapTime } from "../types";
import type { CompareLapMeta } from "../lib/compareLaps";
import { MAX_COMPARE_LAPS } from "../lib/compareLaps";

interface LapPanelProps {
  laps: LapRecord[];
  draftIds: string[];
  comparedIds: string[];
  pickerOpen: boolean;
  referenceId: string | null;
  externalMetas: CompareLapMeta[];
  canAddExternal: boolean;
  onToggleLap: (id: string) => void;
  onSetReference: (id: string) => void;
  onPinLap: (lapId: string, pinned: boolean) => void;
  onLapsRefresh: () => void;
  onAddExternal: () => void;
  onCompare: () => void;
  onReopenPicker: () => void;
}

export function LapPanel({
  laps,
  draftIds,
  comparedIds,
  pickerOpen,
  referenceId,
  externalMetas,
  canAddExternal,
  onToggleLap,
  onSetReference,
  onPinLap,
  onLapsRefresh,
  onAddExternal,
  onCompare,
  onReopenPicker,
}: LapPanelProps) {
  const [expanded, setExpanded] = useState(true);

  const draftExternal = externalMetas.filter((m) => draftIds.includes(m.lapId));
  const comparedExternal = externalMetas.filter((m) =>
    comparedIds.includes(m.lapId),
  );

  if (!pickerOpen) {
    return (
      <div className="lap-panel collapsed lap-panel-summary">
        <div className="lap-panel-summary-header">
          <span>
            {comparedIds.length} lap{comparedIds.length !== 1 ? "s" : ""} compared
          </span>
          <button
            type="button"
            className="secondary lap-panel-reopen"
            onClick={onReopenPicker}
          >
            Change laps
          </button>
        </div>
        <div className="lap-chips">
          {laps
            .filter((lap) => comparedIds.includes(lap.id))
            .map((lap) => {
              const isRef = referenceId === lap.id;
              return (
                <button
                  key={lap.id}
                  type="button"
                  className={`lap-chip ${isRef ? "ref" : ""}`}
                  onClick={() => onSetReference(lap.id)}
                  title={`Lap ${lap.lap_number} · ${formatLapTime(lap.lap_time_ms)}`}
                >
                  <span>L{lap.lap_number}</span>
                  <span className="lap-chip-time">
                    {formatLapTime(lap.lap_time_ms)}
                  </span>
                  {lap.is_best && <span className="tag best">B</span>}
                  {isRef && <span className="tag">Ref</span>}
                </button>
              );
            })}
          {comparedExternal.map((meta) => {
            const isRef = referenceId === meta.lapId;
            return (
              <button
                key={meta.lapId}
                type="button"
                className={`lap-chip external ${isRef ? "ref" : ""}`}
                onClick={() => onSetReference(meta.lapId)}
                title={`${meta.playerName} · ${formatLapTime(meta.lapTimeMs)}`}
              >
                <span className="lap-chip-external-label">
                  {meta.playerName}
                </span>
                <span className="lap-chip-time">
                  {formatLapTime(meta.lapTimeMs)}
                </span>
                {isRef && <span className="tag">Ref</span>}
              </button>
            );
          })}
        </div>
      </div>
    );
  }

  return (
    <div className={`lap-panel ${expanded ? "expanded" : "collapsed"}`}>
      <div className="lap-panel-header-row">
        <button
          type="button"
          className="lap-panel-header"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
        >
          <span>
            {draftIds.length} lap{draftIds.length !== 1 ? "s" : ""} selected
          </span>
          <span className="lap-panel-chevron">{expanded ? "▾" : "▴"}</span>
        </button>
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

      {!expanded && (
        <div className="lap-chips">
          {laps
            .filter((lap) => draftIds.includes(lap.id))
            .map((lap) => {
              const isRef = referenceId === lap.id;
              return (
                <button
                  key={lap.id}
                  type="button"
                  className={`lap-chip ${isRef ? "ref" : ""}`}
                  onClick={() => onToggleLap(lap.id)}
                  title={`Lap ${lap.lap_number} · ${formatLapTime(lap.lap_time_ms)}`}
                >
                  <span>L{lap.lap_number}</span>
                  <span className="lap-chip-time">
                    {formatLapTime(lap.lap_time_ms)}
                  </span>
                  {lap.is_best && <span className="tag best">B</span>}
                  {!lap.valid && <span className="tag invalid">!</span>}
                </button>
              );
            })}
          {draftExternal.map((meta) => {
            const isRef = referenceId === meta.lapId;
            return (
              <button
                key={meta.lapId}
                type="button"
                className={`lap-chip external ${isRef ? "ref" : ""}`}
                onClick={() => onToggleLap(meta.lapId)}
                title={`${meta.playerName} · ${formatLapTime(meta.lapTimeMs)}`}
              >
                <span className="lap-chip-external-label">
                  {meta.playerName}
                </span>
                <span className="lap-chip-time">
                  {formatLapTime(meta.lapTimeMs)}
                </span>
              </button>
            );
          })}
        </div>
      )}

      {expanded && (
        <div className="lap-panel-expanded">
          <p className="muted small">Select up to {MAX_COMPARE_LAPS} laps</p>
          <div className="lap-list-scroll">
            {laps.map((lap) => (
              <label key={lap.id} className="lap-option">
                <input
                  type="checkbox"
                  checked={draftIds.includes(lap.id)}
                  onChange={() => onToggleLap(lap.id)}
                />
                <div>
                  <strong>Lap {lap.lap_number}</strong>
                  <span>{formatLapTime(lap.lap_time_ms)}</span>
                  <div className="lap-tags">
                    {lap.is_best && <span className="tag best">Best</span>}
                    {!lap.valid && <span className="tag invalid">Invalid</span>}
                    {lap.is_pinned && <span className="tag pinned">Pinned</span>}
                    {referenceId === lap.id && <span className="tag">Ref</span>}
                  </div>
                </div>
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
              </label>
            ))}

            {draftExternal.map((meta) => (
              <label key={meta.lapId} className="lap-option external">
                <input
                  type="checkbox"
                  checked={draftIds.includes(meta.lapId)}
                  onChange={() => onToggleLap(meta.lapId)}
                />
                <div>
                  <strong>{meta.playerName}</strong>
                  <span>{formatLapTime(meta.lapTimeMs)}</span>
                  <div className="lap-tags">
                    <span className="tag">External</span>
                    {referenceId === meta.lapId && <span className="tag">Ref</span>}
                  </div>
                </div>
                <div className="lap-actions">
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => onSetReference(meta.lapId)}
                  >
                    Ref
                  </button>
                </div>
              </label>
            ))}
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
      )}

      {!expanded && (
        <div className="lap-panel-footer lap-panel-footer-collapsed">
          <button
            type="button"
            className="lap-panel-compare"
            disabled={draftIds.length === 0}
            onClick={onCompare}
          >
            Compare
          </button>
        </div>
      )}
    </div>
  );
}
