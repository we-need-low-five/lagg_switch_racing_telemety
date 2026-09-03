import type { CSSProperties } from "react";

interface SegmentDeltaStripProps {
  /** Null where the lap could not measure that segment. */
  values: (number | null)[];
  mode: "delta" | "time";
  lapColor: string;
  fullLapTime: string;
  selectedSegment: number | null;
  onSelectSegment: (segment: number | null) => void;
}

function formatDelta(seconds: number): string {
  const sign = seconds > 0 ? "+" : "";
  return `${sign}${seconds.toFixed(3)}s`;
}

function formatTime(seconds: number): string {
  return `${seconds.toFixed(3)}s`;
}

export function SegmentDeltaStrip({
  values,
  mode,
  lapColor,
  fullLapTime,
  selectedSegment,
  onSelectSegment,
}: SegmentDeltaStripProps) {
  const fullSelected = selectedSegment === null;
  const stripStyle = {
    "--segment-lap-color": lapColor,
  } as CSSProperties;

  return (
    <div className="segment-delta-strip" style={stripStyle}>
      <button
        type="button"
        className={`segment-cell full-lap ${fullSelected ? "selected" : ""}`}
        onClick={() => onSelectSegment(null)}
        title={`Full Lap · ${fullLapTime}`}
      >
        <span className="segment-cell-label">Full Lap</span>
        <span className="segment-cell-value time">{fullLapTime}</span>
      </button>
      {values.map((value, i) => (
        <button
          key={i}
          type="button"
          className={`segment-cell ${selectedSegment === i ? "selected" : ""}`}
          onClick={() =>
            onSelectSegment(selectedSegment === i ? null : i)
          }
          title={`S${i + 1}`}
        >
          <span className="segment-cell-label">S{i + 1}</span>
          <span
            className={`segment-cell-value ${
              value == null
                ? "unavailable"
                : mode === "time"
                  ? "time"
                  : value <= 0
                    ? "positive"
                    : "negative"
            }`}
          >
            {value == null
              ? "—"
              : mode === "time"
                ? formatTime(value)
                : formatDelta(value)}
          </span>
        </button>
      ))}
    </div>
  );
}
