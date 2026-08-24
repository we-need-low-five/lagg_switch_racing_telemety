import { useMemo } from "react";
import { lapHasChannel } from "../../lib/accExtras";
import type { DistanceSample } from "../../types";

interface TractionCircleProps {
  samplesByLap: Array<{
    label: string;
    color: string;
    samples: DistanceSample[];
  }>;
  cursorPct?: number | null;
}

const WIDTH = 420;
const HEIGHT = 320;
const PAD = 36;
/** Keep ~250 plotted points regardless of distance-grid density. */
const TARGET_PLOT_POINTS = 250;
/** GT3 envelope is ~2.5G; shared-memory spikes (kerbs/resets) must not set the scale. */
const MAX_PLAUSIBLE_G = 4;

function themeCssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

function nearestSample(
  samples: DistanceSample[],
  pct: number,
): DistanceSample | null {
  if (samples.length === 0) return null;
  let best = samples[0];
  let bestDist = Math.abs(best.distance_pct - pct);
  for (let i = 1; i < samples.length; i++) {
    const d = Math.abs(samples[i].distance_pct - pct);
    if (d < bestDist) {
      best = samples[i];
      bestDist = d;
    }
  }
  return best;
}

function niceExtent(peak: number): number {
  if (!Number.isFinite(peak) || peak <= 0) return 1.5;
  const padded = peak * 1.08;
  const exp = Math.floor(Math.log10(padded));
  const step = 10 ** (exp - 1);
  return Math.ceil(padded / step) * step;
}

export function TractionCircle({
  samplesByLap,
  cursorPct = null,
}: TractionCircleProps) {
  const lapsWithG = useMemo(
    () =>
      samplesByLap.filter(
        (lap) =>
          lapHasChannel(lap.samples, "g_force_x") &&
          lapHasChannel(lap.samples, "g_force_z"),
      ),
    [samplesByLap],
  );

  const { extent, points, cursors } = useMemo(() => {
    let peak = 0;
    const plotted: Array<{
      key: string;
      color: string;
      x: number;
      y: number;
      opacity: number;
    }> = [];

    for (const lap of lapsWithG) {
      for (const s of lap.samples) {
        const lat = s.g_force_x;
        const long = s.g_force_z;
        if (
          lat == null ||
          long == null ||
          !Number.isFinite(lat) ||
          !Number.isFinite(long)
        ) {
          continue;
        }
        const mag = Math.hypot(lat, long);
        if (mag <= MAX_PLAUSIBLE_G) {
          peak = Math.max(peak, mag);
        }
      }
      const stride = Math.max(
        1,
        Math.round(lap.samples.length / TARGET_PLOT_POINTS),
      );
      for (let i = 0; i < lap.samples.length; i += stride) {
        const s = lap.samples[i];
        const lat = s.g_force_x;
        const long = s.g_force_z;
        if (
          lat == null ||
          long == null ||
          !Number.isFinite(lat) ||
          !Number.isFinite(long)
        ) {
          continue;
        }
        const mag = Math.hypot(lat, long);
        if (mag > MAX_PLAUSIBLE_G) continue;
        const drive = Math.max(0, Math.min(1, s.throttle));
        const brake = Math.max(0, Math.min(1, s.brake));
        const opacity = 0.25 + 0.55 * Math.max(drive, brake);
        plotted.push({
          key: `${lap.label}-${i}`,
          color: lap.color,
          x: lat,
          y: long,
          opacity,
        });
      }
    }

    const cursorPts: Array<{
      key: string;
      color: string;
      x: number;
      y: number;
    }> = [];
    if (cursorPct != null) {
      for (const lap of lapsWithG) {
        const s = nearestSample(lap.samples, cursorPct);
        if (!s) continue;
        const lat = s.g_force_x;
        const long = s.g_force_z;
        if (
          lat == null ||
          long == null ||
          !Number.isFinite(lat) ||
          !Number.isFinite(long)
        ) {
          continue;
        }
        const mag = Math.hypot(lat, long);
        if (mag <= MAX_PLAUSIBLE_G) {
          peak = Math.max(peak, mag);
        }
        const scaleTo = mag > MAX_PLAUSIBLE_G ? MAX_PLAUSIBLE_G / mag : 1;
        cursorPts.push({
          key: `cursor-${lap.label}`,
          color: lap.color,
          x: lat * scaleTo,
          y: long * scaleTo,
        });
      }
    }

    return {
      extent: niceExtent(peak),
      points: plotted,
      cursors: cursorPts,
    };
  }, [lapsWithG, cursorPct]);

  if (samplesByLap.length === 0) {
    return <div className="traction-circle empty">Select laps to compare</div>;
  }

  if (lapsWithG.length === 0) {
    return (
      <div className="traction-circle empty">
        No G-force data in selected laps
      </div>
    );
  }

  const inner = Math.min(WIDTH, HEIGHT) - PAD * 2;
  const cx = WIDTH / 2;
  const cy = HEIGHT / 2;
  const scale = inner / 2 / extent;
  const axis = themeCssVar("--chart-axis", "hsl(220 12% 62%)");
  const grid = themeCssVar("--chart-grid", "hsl(250 9% 18%)");

  const toSvg = (lat: number, long: number) => ({
    x: cx + lat * scale,
    // Brake (negative long G) plots upward.
    y: cy + long * scale,
  });

  const ringRadii = [0.5, 1, 1.5, 2, 2.5, 3].filter((g) => g <= extent + 1e-6);

  return (
    <div className="traction-circle">
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} preserveAspectRatio="xMidYMid meet">
        <rect
          x={0}
          y={0}
          width={WIDTH}
          height={HEIGHT}
          fill="var(--track-map-bg)"
          rx={12}
        />

        {ringRadii.map((g) => {
          const r = g * scale;
          return (
            <circle
              key={`ring-${g}`}
              cx={cx}
              cy={cy}
              r={r}
              fill="none"
              stroke={grid}
              strokeWidth={1}
            />
          );
        })}

        <line
          x1={cx - inner / 2}
          y1={cy}
          x2={cx + inner / 2}
          y2={cy}
          stroke={axis}
          strokeWidth={1}
        />
        <line
          x1={cx}
          y1={cy - inner / 2}
          x2={cx}
          y2={cy + inner / 2}
          stroke={axis}
          strokeWidth={1}
        />

        <text x={cx + inner / 2 - 4} y={cy - 6} fill={axis} fontSize={11} textAnchor="end">
          Lat +
        </text>
        <text x={cx - inner / 2 + 4} y={cy - 6} fill={axis} fontSize={11} textAnchor="start">
          Lat −
        </text>
        <text x={cx + 6} y={cy - inner / 2 + 12} fill={axis} fontSize={11}>
          Brake
        </text>
        <text x={cx + 6} y={cy + inner / 2 - 4} fill={axis} fontSize={11}>
          Accel
        </text>
        <text
          x={cx + inner / 2 - 4}
          y={cy + inner / 2 - 4}
          fill={axis}
          fontSize={11}
          textAnchor="end"
        >
          ±{extent.toFixed(1)} G
        </text>

        {points.map((p) => {
          const { x, y } = toSvg(p.x, p.y);
          return (
            <circle
              key={p.key}
              cx={x}
              cy={y}
              r={2}
              fill={p.color}
              opacity={p.opacity}
            />
          );
        })}

        {cursors.map((p) => {
          const { x, y } = toSvg(p.x, p.y);
          return (
            <circle
              key={p.key}
              cx={x}
              cy={y}
              r={6}
              fill="var(--track-map-cursor-fill)"
              stroke={p.color}
              strokeWidth={2}
            />
          );
        })}
      </svg>
      {lapsWithG.length > 1 && (
        <div className="traction-circle-legend">
          {lapsWithG.map((lap) => (
            <span key={lap.label} className="traction-circle-legend-item">
              <span
                className="traction-circle-swatch"
                style={{ background: lap.color }}
              />
              {lap.label}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
