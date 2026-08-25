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
import { ConfirmDialog } from "../components/ConfirmDialog";
import { OverflowMenu } from "../components/OverflowMenu";
import { formatCarName } from "../lib/formatCarName";
import { sessionBundleFileName } from "../lib/sessionBundleName";
import type { RecordingStatus, SessionRecord } from "../types";
import { formatLapTime, gameLabel } from "../types";

export function Sessions() {
  const navigate = useNavigate();
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<SessionRecord | null>(null);
  const [deleting, setDeleting] = useState(false);

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

  async function handleExport(session: SessionRecord) {
    try {
      setError(null);
      const path = await save({
        defaultPath: sessionBundleFileName(session),
        filters: [{ name: "SimTelemetry Bundle", extensions: ["stb"] }],
      });
      if (!path) return;
      await exportSessionBundle(session.id, path);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleImport() {
    try {
      setError(null);
      const path = await open({
        filters: [{ name: "SimTelemetry Bundle", extensions: ["stb"] }],
        multiple: false,
      });
      if (!path || Array.isArray(path)) return;
      const sessionId = await importSessionBundle(path);
      await refresh();
      navigate(`/sessions/${sessionId}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function confirmDelete() {
    if (!pendingDelete || deleting) return;
    try {
      setDeleting(true);
      setError(null);
      await deleteSession(pendingDelete.id);
      setPendingDelete(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setDeleting(false);
    }
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
              try {
                setError(null);
                await setRecordingPaused(!status.paused);
                await refresh();
              } catch (e) {
                setError(String(e));
              }
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
            <p>{formatCarName(session.car)}</p>
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
                    onClick: () => handleExport(session),
                  },
                  {
                    label: "Delete",
                    onClick: () => setPendingDelete(session),
                    danger: true,
                  },
                ]}
              />
            </div>
          </article>
        ))}
      </div>
      </div>

      <ConfirmDialog
        open={pendingDelete != null}
        title="Delete session?"
        message={
          pendingDelete
            ? `Delete ${pendingDelete.track || "this session"} (${formatCarName(pendingDelete.car)}) and all its lap data? Your top 3 laps stay on the leaderboard. This cannot be undone.`
            : ""
        }
        confirmLabel={deleting ? "Deleting…" : "Delete"}
        danger
        onCancel={() => {
          if (!deleting) setPendingDelete(null);
        }}
        onConfirm={confirmDelete}
      />
    </div>
  );
}
