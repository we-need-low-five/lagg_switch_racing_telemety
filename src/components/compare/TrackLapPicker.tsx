import type { GameId } from "../../types";
import type { LeaderboardTrackOption, TrackLapOption } from "../../types";
import { gameLabel, formatLapTime } from "../../types";
import {
  MAX_COMPARE_LAPS,
  toggleSelectedId,
} from "../../lib/compareLaps";

interface TrackLapPickerProps {
  games: GameId[];
  tracks: LeaderboardTrackOption[];
  laps: TrackLapOption[];
  selectedGame: GameId | "";
  selectedTrackKey: string;
  selectedIds: string[];
  loadingGames: boolean;
  loadingLaps: boolean;
  onGameChange: (game: GameId) => void;
  onTrackChange: (trackKey: string) => void;
  onSelectedIdsChange: (ids: string[]) => void;
  onCompare: () => void;
}

function trackOptionKey(track: LeaderboardTrackOption): string {
  return `${track.track_id}|${track.track}`;
}

export function TrackLapPicker({
  games,
  tracks,
  laps,
  selectedGame,
  selectedTrackKey,
  selectedIds,
  loadingGames,
  loadingLaps,
  onGameChange,
  onTrackChange,
  onSelectedIdsChange,
  onCompare,
}: TrackLapPickerProps) {
  function toggleLap(lapId: string) {
    onSelectedIdsChange(toggleSelectedId(selectedIds, lapId));
  }

  return (
    <section className="settings-panel global-compare-picker">
      <h2 className="fuel-calc-section-title">Select laps</h2>
      <p className="muted small">
        Choose up to {MAX_COMPARE_LAPS} valid laps on the same track, including
        leaderboard laps kept after a session is deleted.
      </p>

      <div className="leaderboard-filters">
        <label className="form-field">
          <span className="form-label">Game</span>
          <select
            value={selectedGame}
            onChange={(e) => onGameChange(e.target.value as GameId)}
            disabled={loadingGames || games.length === 0}
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
            onChange={(e) => onTrackChange(e.target.value)}
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

      {loadingLaps && <p className="muted">Loading laps…</p>}

      {!loadingLaps && selectedTrackKey && laps.length === 0 && (
        <p className="muted">No valid laps for this track.</p>
      )}

      {!loadingLaps && laps.length > 0 && (
        <div className="track-lap-list track-lap-list-picker">
          {laps.map((lap) => {
            const checked = selectedIds.includes(lap.lap_id);
            const disabled = !checked && selectedIds.length >= MAX_COMPARE_LAPS;
            return (
              <label key={lap.lap_id} className="track-lap-check-row">
                <input
                  type="checkbox"
                  checked={checked}
                  disabled={disabled}
                  onChange={() => toggleLap(lap.lap_id)}
                />
                <span className="track-lap-row-main">
                  <strong>{lap.player_name}</strong>
                  <span>{formatLapTime(lap.lap_time_ms)}</span>
                </span>
                <span className="muted small">{lap.car}</span>
                <span className="muted small">
                  {new Date(lap.started_at).toLocaleDateString()}
                </span>
              </label>
            );
          })}
        </div>
      )}

      <div className="compare-picker-footer">
        <p className="muted small">
          {selectedIds.length} of {MAX_COMPARE_LAPS} laps selected
        </p>
        <button
          type="button"
          className="lap-panel-compare"
          disabled={selectedIds.length === 0}
          onClick={onCompare}
        >
          Compare
        </button>
      </div>
    </section>
  );
}
