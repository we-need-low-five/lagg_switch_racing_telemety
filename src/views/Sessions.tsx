import { useCallback, useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  deleteSession,
  exportSessionBundle,
  getRecordingStatus,
  importSessionBundle,
  listSessions,
  setRecordingPaused,
} from "../api";
import { OverflowMenu } from "../components/OverflowMenu";
import type { RecordingStatus, SessionRecord } from "../types";
import { formatLapTime, gameLabel } from "../types";

export function Sessions() {
  const navigate = useNavigate();
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const [rows, recording] = await Promise.all([
        listSessions(),
        getRecordingStatus(),
      ]);
      setSessions(rows);
      setStatus(recording);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 3000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  async function handleExport(sessionId: string) {
    const path = await save({
      defaultPath: "session.stb",
      filters: [{ name: "SimTelemetry Bundle", extensions: ["stb"] }],
    });
    if (!path) return;
    await exportSessionBundle(sessionId, path);
    await refresh();
  }

  async function handleImport() {
    const path = await open({
      filters: [{ name: "SimTelemetry Bundle", extensions: ["stb"] }],
      multiple: false,
    });
    if (!path || Array.isArray(path)) return;
    const sessionId = await importSessionBundle(path);
    await refresh();
    navigate(`/sessions/${sessionId}`);
  }

  async function handleDelete(sessionId: string) {
    const confirmed = window.confirm(
      "Delete this session and all its lap data? This cannot be undone.",
    );
    if (!confirmed) return;
    await deleteSession(sessionId);
    await refresh();
  }

  return (
    <div className="page">
      <div className="page-inner">
        <header className="page-header">
        <div>
          <h1>Sessions</h1>
          <p className="subtitle">
            Auto-recording from supported sims. Data stored locally.
          </p>
        </div>
        <div className="header-actions">
          <button type="button" className="secondary" onClick={handleImport}>
            Import Bundle
          </button>
          <button type="button" onClick={refresh}>
            Refresh
          </button>
        </div>
      </header>

      {status && (
        <div className={`status-banner ${status.active ? "active" : ""}`}>
          <div>
            <strong>
              {status.active
                ? `Recording ${status.game ? gameLabel(status.game) : "game"}`
                : "Waiting for game"}
            </strong>
            {status.track && <span> — {status.track}</span>}
          </div>
          <div className="status-meta">
            Lap {status.current_lap} · {status.samples_recorded.toLocaleString()} samples
            {status.paused && " · Paused"}
          </div>
          <button
            type="button"
            className="secondary"
            onClick={async () => {
              await setRecordingPaused(!status.paused);
              await refresh();
            }}
          >
            {status.paused ? "Resume" : "Pause"}
          </button>
        </div>
      )}

      {loading && <p className="muted">Loading sessions…</p>}
      {error && <p className="error">{error}</p>}

      {!loading && sessions.length === 0 && (
        <div className="empty-state">
          <h2>No sessions yet</h2>
          <p>Launch ACC, AC, LMU, or F1 25 and drive — recording starts automatically.</p>
          <Link to="/settings">Open settings</Link>
        </div>
      )}

      <div className="session-grid">
        {sessions.map((session) => (
          <article key={session.id} className="session-card">
            <div className="session-card-top">
              <span className="badge">{gameLabel(session.game)}</span>
              <span className="muted">
                {new Date(session.started_at).toLocaleString()}
              </span>
            </div>
            <h3>{session.track || "Unknown track"}</h3>
            <p>{session.car}</p>
            <div className="session-stats">
              <span>{session.lap_count} laps</span>
              <span>
                Best:{" "}
                {session.best_lap_time_ms
                  ? formatLapTime(session.best_lap_time_ms)
                  : "—"}
              </span>
            </div>
            <div className="card-actions">
              <button
                type="button"
                onClick={() => navigate(`/sessions/${session.id}`)}
              >
                Review session
              </button>
              <OverflowMenu
                items={[
                  {
                    label: "Export",
                    onClick: () => handleExport(session.id),
                  },
                  {
                    label: "Delete",
                    onClick: () => handleDelete(session.id),
                    danger: true,
                  },
                ]}
              />
            </div>
          </article>
        ))}
      </div>
      </div>
    </div>
  );
}
