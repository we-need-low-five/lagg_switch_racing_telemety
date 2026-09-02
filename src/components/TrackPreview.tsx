import { useEffect, useMemo, useState } from "react";
import {
  loadTrackLayout,
  makeTrackProjector,
  outlinePathFrom,
  resolveTrackId,
  type TrackLayout,
} from "../lib/trackLayout";

const WIDTH = 320;
const HEIGHT = 96;
const PAD = 6;

interface TrackPreviewProps {
  trackId?: string | null;
  trackName?: string | null;
}

/** Static outline of a track, for session cards. No cursor, no samples. */
export function TrackPreview({ trackId, trackName }: TrackPreviewProps) {
  const [layout, setLayout] = useState<TrackLayout | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const id = resolveTrackId(trackId, trackName);
    if (!id) {
      setLayout(null);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    loadTrackLayout(id)
      .then((found) => {
        if (!cancelled) setLayout(found);
      })
      .catch(() => {
        if (!cancelled) setLayout(null);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [trackId, trackName]);

  const shape = useMemo(() => {
    if (!layout || layout.points.length < 2) return null;
    const project = makeTrackProjector(layout.points, {
      width: WIDTH,
      height: HEIGHT,
      pad: PAD,
    });
    const projected = layout.points.map(([x, y]) => project(x, y));
    return { path: outlinePathFrom(projected), start: projected[0] };
  }, [layout]);

  return (
    <div className="track-preview">
      {shape ? (
        <svg
          viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={`${layout?.name ?? trackName ?? "Track"} layout`}
        >
          <path className="track-preview-outline" d={shape.path} />
          <circle
            className="track-preview-start"
            cx={shape.start.x}
            cy={shape.start.y}
            r={3.5}
          />
        </svg>
      ) : (
        !loading && <span className="track-preview-empty">No layout</span>
      )}
    </div>
  );
}
