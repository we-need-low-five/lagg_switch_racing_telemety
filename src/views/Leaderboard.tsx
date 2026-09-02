import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  getLeaderboard,
  listLeaderboardGames,
  listLeaderboardTracks,
} from "../api";
import { formatCarName } from "../lib/formatCarName";
import type { GameId, LeaderboardEntry, LeaderboardTrackOption } from "../types";
import { formatLapTime, gameLabel } from "../types";

function trackOptionKey(track: LeaderboardTrackOption): string {
  return `${track.track_id}|${track.track}`;
}

function parseTrackKey(key: string): { trackId: string; trackName: string } {
  const idx = key.indexOf("|");
  if (idx === -1) return { trackId: key, trackName: key };
  return {
    trackId: key.slice(0, idx),
    trackName: key.slice(idx + 1),
  };
}

export function Leaderboard() {
  const navigate = useNavigate();
  const [games, setGames] = useState<GameId[]>([]);
  const [tracks, setTracks] = useState<LeaderboardTrackOption[]>([]);
  const [entries, setEntries] = useState<LeaderboardEntry[]>([]);
  const [selectedGame, setSelectedGame] = useState<GameId | "">("");
  const [selectedTrackKey, setSelectedTrackKey] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadGames = useCallback(async () => {
    try {
      setError(null);
      const rows = await listLeaderboardGames();
      setGames(rows);
      setSelectedGame((prev) => {
        if (prev && rows.includes(prev)) return prev;
        return rows[0] ?? "";
      });
    } catch (e) {
      setError(String(e));
      setGames([]);
      setSelectedGame("");
    }
  }, []);

  useEffect(() => {
    loadGames().finally(() => setLoading(false));
  }, [loadGames]);

  useEffect(() => {
    if (!selectedGame) {
      setTracks([]);
      setSelectedTrackKey("");
      setEntries([]);
      return;
    }
    let cancelled = false;
    listLeaderboardTracks(selectedGame)
      .then((rows) => {
        if (cancelled) return;
        setTracks(rows);
        const keys = rows.map(trackOptionKey);
        setSelectedTrackKey((prev) => {
          if (prev && keys.includes(prev)) return prev;
          return keys[0] ?? "";
        });
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedGame]);

  useEffect(() => {
    if (!selectedGame || !selectedTrackKey) {
      setEntries([]);
      return;
    }
    const { trackId, trackName } = parseTrackKey(selectedTrackKey);
    let cancelled = false;
    getLeaderboard(selectedGame, trackId, trackName)
      .then((rows) => {
        if (!cancelled) setEntries(rows);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedGame, selectedTrackKey]);

  return (
    <div className="page">
      <div className="page-inner">
        <header className="page-header">
          <div>
            <h1>Leaderboard</h1>
            <p className="subtitle">
              Top 3 valid laps per player, with full telemetry kept for
              comparison even if you delete the session.
            </p>
          </div>
        </header>

        {loading && <p className="muted">Loading…</p>}
        {error && <p className="error">{error}</p>}

        {!loading && games.length === 0 && (
          <div className="empty-state">
            <h2>No leaderboard data yet</h2>
            <p>
              Record or import valid laps to see rankings. Your top 3 times stay
              after you delete a session.
            </p>
          </div>
        )}

        {games.length > 0 && (
          <div className="leaderboard-filters settings-panel">
            <label className="form-field">
              <span className="form-label">Game</span>
              <select
                value={selectedGame}
                onChange={(e) => setSelectedGame(e.target.value as GameId)}
              >
                {games.map((game) => (
                  <option key={game} value={game}>
                    {gameLabel(game)}
                  </option>
                ))}
              </select>
            </label>
            <label className="form-field">
              <span className="form-label">Track</span>
              <select
                value={selectedTrackKey}
                onChange={(e) => setSelectedTrackKey(e.target.value)}
                disabled={tracks.length === 0}
              >
                {tracks.map((track) => (
                  <option key={trackOptionKey(track)} value={trackOptionKey(track)}>
                    {track.track}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}

        {games.length > 0 && tracks.length === 0 && selectedGame && (
          <p className="muted">No tracks with valid laps for this game.</p>
        )}

        {entries.length > 0 && (
          <div className="leaderboard-table-wrap settings-panel">
            <table className="leaderboard-table">
              <thead>
                <tr>
                  <th>Rank</th>
                  <th>Player</th>
                  <th>Car</th>
                  <th>Lap time</th>
                  <th>Valid</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry) => (
                    <tr
                      key={`${entry.session_id}-${entry.lap_id}`}
                      className="leaderboard-row"
                      onClick={() =>
                        navigate("/compare", {
                          state: {
                            game: selectedGame,
                            trackKey: selectedTrackKey,
                            lapIds: [entry.lap_id],
                          },
                        })
                      }
                      title="Compare this lap"
                    >
                      <td>{entry.rank}</td>
                      <td>{entry.player_name}</td>
                      <td>{entry.car ? formatCarName(entry.car) : "—"}</td>
                      <td>{formatLapTime(entry.lap_time_ms)}</td>
                      <td>
                        <span className="tag">
                          {entry.valid ? "Valid" : "Invalid"}
                        </span>
                      </td>
                    </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {games.length > 0 &&
          tracks.length > 0 &&
          selectedTrackKey &&
          entries.length === 0 &&
          !loading && (
            <p className="muted">No entries for this game and track.</p>
          )}
      </div>
    </div>
  );
}
