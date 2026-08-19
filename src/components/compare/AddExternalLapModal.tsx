import { formatLapTime } from "../../types";
import type { TrackLapOption } from "../../types";
import { MAX_COMPARE_LAPS } from "../../lib/compareLaps";

interface AddExternalLapModalProps {
  open: boolean;
  onClose: () => void;
  laps: TrackLapOption[];
  loading: boolean;
  selectedIds: string[];
  currentSessionId: string;
  onSelect: (lapId: string) => void;
}

export function AddExternalLapModal({
  open,
  onClose,
  laps,
  loading,
  selectedIds,
  currentSessionId,
  onSelect,
}: AddExternalLapModalProps) {
  if (!open) return null;

  const atCap = selectedIds.length >= MAX_COMPARE_LAPS;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal-panel track-lap-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <h2>Add lap from track</h2>
          <button type="button" className="secondary" onClick={onClose}>
            Close
          </button>
        </div>

        {loading && <p className="muted">Loading laps…</p>}
        {!loading && laps.length === 0 && (
          <p className="muted">No valid laps found for this track.</p>
        )}

        {!loading && laps.length > 0 && (
          <div className="track-lap-list">
            {laps.map((lap) => {
              const selected = selectedIds.includes(lap.lap_id);
              const disabled = selected || atCap;
              const isCurrentSession = lap.session_id === currentSessionId;
              return (
                <button
                  key={lap.lap_id}
                  type="button"
                  className="track-lap-row"
                  disabled={disabled}
                  onClick={() => {
                    if (!disabled) {
                      onSelect(lap.lap_id);
                      onClose();
                    }
                  }}
                >
                  <span className="track-lap-row-main">
                    <strong>{lap.player_name}</strong>
                    <span>{formatLapTime(lap.lap_time_ms)}</span>
                  </span>
                  <span className="track-lap-row-meta muted small">
                    {lap.car}
                    {isCurrentSession && (
                      <span className="tag">This session</span>
                    )}
                    {selected && <span className="tag">Selected</span>}
                  </span>
                </button>
              );
            })}
          </div>
        )}

        {atCap && (
          <p className="muted small">Maximum {MAX_COMPARE_LAPS} laps selected.</p>
        )}
      </div>
    </div>
  );
}
