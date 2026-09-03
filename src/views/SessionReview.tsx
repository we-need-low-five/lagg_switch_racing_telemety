import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { getSession, listLaps } from "../api";
import { displaySectorTimes } from "../lib/compareLaps";
import { findShortLaps, formatDistanceKm } from "../lib/lapDistance";
import { findPartialTraces, formatCoveragePct } from "../lib/lapTrace";
import {
  computeSessionLapStats,
  computeStintStatsMap,
} from "../lib/sessionLapStats";
import {
  formatStintBreak,
  lapStint,
  lapsWithStintSeparators,
  sessionHasMultipleStints,
} from "../lib/stints";
import {
  formatFuelLiters,
  fuelUnitLabel,
  usePreferences,
} from "../lib/preferences";
import type { LapRecord, SessionRecord } from "../types";
import { formatLapTime, gameLabel } from "../types";

function formatSectorMs(ms: number | null | undefined): string {
  if (ms == null) return "—";
  return (ms / 1000).toFixed(3);
}

function formatDeltaMs(deltaMs: number | null): string {
  if (deltaMs == null) return "—";
  const sign = deltaMs > 0 ? "+" : "";
  return `${sign}${(deltaMs / 1000).toFixed(3)}`;
}

export function SessionReview() {
  const { sessionId } = useParams();
  const navigate = useNavigate();
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [laps, setLaps] = useState<LapRecord[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [prefs] = usePreferences();

  const refresh = useCallback(async () => {
    if (!sessionId) return;
    try {
      setError(null);
      const [sessionRow, lapRows] = await Promise.all([
        getSession(sessionId),
        listLaps(sessionId),
      ]);
      setSession(sessionRow);
      setLaps(lapRows);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const bestLap = useMemo(
    () => laps.find((l) => l.is_best) ?? laps[0],
    [laps],
  );
  const shortLapIds = useMemo(() => findShortLaps(laps), [laps]);
  const partialTraceIds = useMemo(() => findPartialTraces(laps), [laps]);
  const multiStint = useMemo(() => sessionHasMultipleStints(laps), [laps]);
  const stintStats = useMemo(
    () => computeStintStatsMap(laps, session?.game),
    [laps, session?.game],
  );

  const lapStats = useMemo(
    () => computeSessionLapStats(laps, session?.game),
    [laps, session?.game],
  );

  function toggleSelect(id: string) {
    setSelectedIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  }

  function openAnalysis(lapIds: string[], referenceId?: string) {
    if (!sessionId || lapIds.length === 0) return;
    const ref = referenceId ?? bestLap?.id ?? lapIds[0];
    navigate(`/compare/${sessionId}`, {
      state: { lapIds, referenceId: ref },
    });
  }

  function handleRowClick(lap: LapRecord) {
    openAnalysis([lap.id], bestLap?.id);
  }

  function handleCompareSelected() {
    if (selectedIds.length < 2) return;
    openAnalysis(selectedIds, bestLap?.id);
  }

  return (
    <div className="page">
      <div className="page-inner">
        <header className="page-header">
          <div>
            <Link to="/" className="back-link">← Sessions</Link>
            <h1>Review Session</h1>
            {session && (
              <p className="subtitle">
                {gameLabel(session.game)} · {session.track} · {session.car}
              </p>
            )}
          </div>
          {selectedIds.length >= 2 && (
            <div className="header-actions">
              <button type="button" onClick={handleCompareSelected}>
                Compare selected ({selectedIds.length})
              </button>
            </div>
          )}
        </header>

        {loading && <p className="muted">Loading laps…</p>}
        {error && <p className="error">{error}</p>}

        {!loading && laps.length === 0 && (
          <div className="empty-state">
            <h2>No laps recorded</h2>
            <p>This session has no lap data yet.</p>
          </div>
        )}

        {laps.length > 0 && (
          <>
            <div className="review-summary-cards">
              <div className="review-summary-card">
                <span className="review-summary-label">Best lap</span>
                <span className="review-summary-value">
                  {lapStats.bestLap
                    ? formatLapTime(lapStats.bestLap.lap_time_ms)
                    : "—"}
                </span>
                <span className="review-summary-meta muted">
                  {lapStats.bestLap
                    ? `Lap ${lapStats.bestLap.lap_number}`
                    : "No valid lap"}
                </span>
              </div>
              <div className="review-summary-card">
                <span className="review-summary-label">Optimal lap</span>
                <span className="review-summary-value">
                  {lapStats.optimalLapMs != null
                    ? formatLapTime(lapStats.optimalLapMs)
                    : "—"}
                </span>
                <span className="review-summary-meta muted">
                  Best S1 + S2 + S3
                </span>
              </div>
              <div className="review-summary-card">
                <span className="review-summary-label">Average lap</span>
                <span className="review-summary-value">
                  {lapStats.averageLapMs != null
                    ? formatLapTime(lapStats.averageLapMs)
                    : "—"}
                </span>
                <span className="review-summary-meta muted">
                  {lapStats.averageLapCount > 0
                    ? `${lapStats.averageLapCount} lap${lapStats.averageLapCount === 1 ? "" : "s"}`
                    : "No laps"}
                </span>
              </div>
              <div className="review-summary-card">
                <span className="review-summary-label">Average valid lap</span>
                <span className="review-summary-value">
                  {lapStats.averageValidLapMs != null
                    ? formatLapTime(lapStats.averageValidLapMs)
                    : "—"}
                </span>
                <span className="review-summary-meta muted">
                  {lapStats.validLapCount > 0
                    ? `${lapStats.validLapCount} valid lap${lapStats.validLapCount === 1 ? "" : "s"}`
                    : "No valid laps"}
                </span>
              </div>
              <div className="review-summary-card">
                <span className="review-summary-label">Average fuel</span>
                <span className="review-summary-value">
                  {lapStats.averageFuelL != null
                    ? `${formatFuelLiters(lapStats.averageFuelL, prefs.fuelUnit)} ${fuelUnitLabel(prefs.fuelUnit)}`
                    : "—"}
                </span>
                <span className="review-summary-meta muted">
                  {lapStats.averageFuelLapCount > 0
                    ? `${lapStats.averageFuelLapCount} valid lap${lapStats.averageFuelLapCount === 1 ? "" : "s"}`
                    : "No fuel data"}
                </span>
              </div>
            </div>

            <div className="review-table-wrap">
            <table className="review-table">
              <thead>
                <tr>
                  <th className="review-col-check" />
                  <th>Lap</th>
                  <th>Time</th>
                  <th>S1</th>
                  <th>S2</th>
                  <th>S3</th>
                  <th>Δ Best</th>
                  <th>Compound</th>
                  <th>TC</th>
                  <th>ABS</th>
                  <th>Fuel used</th>
                </tr>
              </thead>
              <tbody>
                {lapsWithStintSeparators(laps).map((row) => {
                  if (row.kind === "separator") {
                    const st = stintStats.get(row.stint);
                    return (
                      <tr key={`stint-${row.stint}`} className="stint-separator">
                        <td colSpan={11}>
                          {row.phaseLabel
                            ? `${row.phaseLabel} · Stint ${row.stint}`
                            : `Stint ${row.stint}`}
                          {st && (
                            <span className="stint-break">
                              {" "}
                              · {st.lapCount} lap{st.lapCount !== 1 ? "s" : ""}
                              {st.best.bestLap &&
                                ` · best ${formatLapTime(st.best.bestLap.lap_time_ms)}`}
                              {st.best.averageValidLapMs != null &&
                                ` · avg ${formatLapTime(st.best.averageValidLapMs)}`}
                            </span>
                          )}
                          {row.breakS != null && (
                            <span className="stint-break">
                              {" "}
                              · {formatStintBreak(row.breakS)} break
                            </span>
                          )}
                        </td>
                      </tr>
                    );
                  }
                  const lap = row.lap;
                  const sectors = displaySectorTimes(
                    lap.sectors,
                    lap.lap_time_ms,
                    session?.game,
                  );
                  const deltaMs =
                    bestLap && lap.id !== bestLap.id
                      ? lap.lap_time_ms - bestLap.lap_time_ms
                      : lap.is_best
                        ? 0
                        : null;
                  // Partial as well as invalid: neither counts towards the
                  // session's times, so neither reads as an ordinary lap.
                  const rowClass =
                    lap.valid && !partialTraceIds.has(lap.id)
                      ? ""
                      : "invalid-lap";

                  return (
                    <tr
                      key={lap.id}
                      className={`review-row ${rowClass}`}
                      onClick={() => handleRowClick(lap)}
                    >
                      <td
                        className="review-col-check"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <input
                          type="checkbox"
                          checked={selectedIds.includes(lap.id)}
                          onChange={() => toggleSelect(lap.id)}
                          aria-label={`Select lap ${lap.lap_number}`}
                        />
                      </td>
                      <td>
                        {multiStint
                          ? `S${lapStint(lap)}·${lap.lap_number}`
                          : lap.lap_number}
                        {lap.is_best && (
                          <span className="tag best review-inline-tag">B</span>
                        )}
                        {lap.is_pinned && (
                          <span className="tag pinned review-inline-tag">★</span>
                        )}
                        {shortLapIds.has(lap.id) && (
                          <span
                            className="tag short-lap review-inline-tag"
                            title={`Covered ${formatDistanceKm(
                              lap.lap_distance_m ?? 0,
                            )} — less of the track than a full lap here, so its time and its trace do not compare with one`}
                          >
                            Short
                          </span>
                        )}
                        {partialTraceIds.has(lap.id) && (
                          <span
                            className="tag partial-trace review-inline-tag"
                            title={`Only ${formatCoveragePct(
                              lap.trace_coverage ?? 0,
                            )} of this lap was recorded — the trace is a fragment stretched over the whole chart, so the lap does not count towards the session's times`}
                          >
                            Partial
                          </span>
                        )}
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
                          sectors.s1_ms != null &&
                          lapStats.bestS1Ms != null &&
                          sectors.s1_ms === lapStats.bestS1Ms
                            ? "time-best"
                            : undefined
                        }
                      >
                        {formatSectorMs(sectors.s1_ms)}
                      </td>
                      <td
                        className={
                          sectors.s2_ms != null &&
                          lapStats.bestS2Ms != null &&
                          sectors.s2_ms === lapStats.bestS2Ms
                            ? "time-best"
                            : undefined
                        }
                      >
                        {formatSectorMs(sectors.s2_ms)}
                      </td>
                      <td
                        className={
                          sectors.s3_ms != null &&
                          lapStats.bestS3Ms != null &&
                          sectors.s3_ms === lapStats.bestS3Ms
                            ? "time-best"
                            : undefined
                        }
                      >
                        {formatSectorMs(sectors.s3_ms)}
                      </td>
                      <td
                        className={
                          deltaMs != null && deltaMs > 0
                            ? "delta-negative"
                            : deltaMs != null && deltaMs < 0
                              ? "delta-positive"
                              : ""
                        }
                      >
                        {formatDeltaMs(deltaMs)}
                      </td>
                      <td>{lap.tyre_compound?.trim() || "—"}</td>
                      <td>{lap.tc_level ?? "—"}</td>
                      <td>{lap.abs_level ?? "—"}</td>
                      <td>{formatFuelLiters(lap.fuel_used_l, prefs.fuelUnit)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
