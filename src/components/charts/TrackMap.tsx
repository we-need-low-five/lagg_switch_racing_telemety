import { useMemo } from "react";
import { lapTimeDeltaAtPct } from "../../lib/chartAlign";
import type { DistanceSample } from "../../types";

function themeCssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}
import {
  buildPathMetrics,
  makeTrackProjector,
  outlinePathFrom,
  type PathMetrics,
  type TrackLayout,
} from "../../lib/trackLayout";

interface TrackMapProps {
  layout: TrackLayout | null;
  layoutLoading?: boolean;
  samplesByLap: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  mode: "speed" | "delta";
  reference?: DistanceSample[];
  cursorPct?: number | null;
  onCursorMove?: (pct: number | null) => void;
}

const WIDTH = 420;
const HEIGHT = 260;
const PAD = 20;

function outlinePath(metrics: PathMetrics) {
  return outlinePathFrom(metrics.points);
}

/** Convert a mouse event into SVG viewBox coordinates (handles letterboxing). */
function clientToSvgPoint(
  svg: SVGSVGElement,
  clientX: number,
  clientY: number,
): { x: number; y: number } | null {
  const ctm = svg.getScreenCTM();
  if (!ctm) return null;
  const pt = svg.createSVGPoint();
  pt.x = clientX;
  pt.y = clientY;
  const local = pt.matrixTransform(ctm.inverse());
  return { x: local.x, y: local.y };
}

export function TrackMap({
  layout,
  layoutLoading = false,
  samplesByLap,
  mode,
  reference = [],
  cursorPct,
  onCursorMove,
}: TrackMapProps) {
  const primary = samplesByLap[0]?.samples ?? [];

  const { metrics, projectSample } = useMemo(() => {
    if (!layout || layout.points.length < 2) {
      return { metrics: null, projectSample: null };
    }
    const project = makeTrackProjector(layout.points, {
      width: WIDTH,
      height: HEIGHT,
      pad: PAD,
    });
    const pathMetrics = buildPathMetrics(layout.points, project);
    return {
      metrics: pathMetrics,
      projectSample: (sample: DistanceSample) =>
        pathMetrics.pointAtPct(sample.distance_pct),
    };
  }, [layout]);

  if (layoutLoading) {
    return <div className="track-map empty">Loading track layout…</div>;
  }

  if (!layout || !metrics || !projectSample) {
    return (
      <div className="track-map empty">
        {primary.length === 0
          ? "No lap data"
          : "Track layout not available for this circuit"}
      </div>
    );
  }

  const colorForPoint = (sample: DistanceSample) => {
    if (mode === "speed") {
      const t = Math.min(1, sample.speed_mps / 80);
      const r = Math.round(56 + t * 199);
      const g = Math.round(189 - t * 100);
      const b = Math.round(248 - t * 120);
      return `rgb(${r},${g},${b})`;
    }
    const delta = lapTimeDeltaAtPct(sample, reference);
    if (delta < -0.05) return themeCssVar("--color-positive", "hsl(142 55% 55%)");
    if (delta > 0.05) return themeCssVar("--color-negative", "hsl(0 65% 62%)");
    return themeCssVar("--text-muted", "hsl(220 12% 62%)");
  };

  const cursorPoint =
    cursorPct == null ? null : metrics.pointAtPct(cursorPct);
  const speedStride = Math.max(1, Math.round(primary.length / 125));

  return (
    <div className="track-map">
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        preserveAspectRatio="xMidYMid meet"
        onMouseMove={(e) => {
          if (!onCursorMove) return;
          const local = clientToSvgPoint(
            e.currentTarget as SVGSVGElement,
            e.clientX,
            e.clientY,
          );
          if (!local) return;
          onCursorMove(metrics.pctAtSvg(local.x, local.y));
        }}
        onMouseLeave={() => onCursorMove?.(null)}
      >
        <rect
          x={0}
          y={0}
          width={WIDTH}
          height={HEIGHT}
          fill="var(--track-map-bg)"
          rx={12}
        />
        <path
          d={outlinePath(metrics)}
          fill="none"
          stroke="var(--track-map-outline)"
          strokeWidth={10}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          d={outlinePath(metrics)}
          fill="none"
          stroke="var(--track-map-outline-inner)"
          strokeWidth={2}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        {mode === "speed" &&
          primary
            .filter((_, i) => i % speedStride === 0)
            .map((s, i) => {
              const p = projectSample(s);
              return (
                <circle
                  key={i}
                  cx={p.x}
                  cy={p.y}
                  r={3}
                  fill={colorForPoint(s)}
                />
              );
            })}
        {mode === "delta" &&
          samplesByLap.map((lap) => {
            const stride = Math.max(1, Math.round(lap.samples.length / 83));
            return lap.samples
              .filter((_, i) => i % stride === 0)
              .map((s, i) => {
                const p = projectSample(s);
                return (
                  <circle
                    key={`${lap.label}-${i}`}
                    cx={p.x}
                    cy={p.y}
                    r={2.5}
                    fill={colorForPoint(s)}
                    opacity={0.9}
                  />
                );
              });
          })}
        {cursorPoint && (
          <circle
            cx={cursorPoint.x}
            cy={cursorPoint.y}
            r={6}
            fill="var(--track-map-cursor-fill)"
            stroke="var(--track-map-cursor-stroke)"
            strokeWidth={2}
          />
        )}
      </svg>
    </div>
  );
}
