export interface TrackLayout {
  id: string;
  name: string;
  points: [number, number][];
  /** Lap distance % where S2 and S3 begin. */
  sector_splits?: [number, number];
}

const DISPLAY_NAME_TO_ID: Record<string, string> = {
  Barcelona: "barcelona",
  "Brands Hatch": "brands_hatch",
  "Circuit of the Americas": "cota",
  "Donington Park": "donington",
  Hungaroring: "hungaroring",
  Imola: "imola",
  Indianapolis: "indianapolis",
  Kyalami: "kyalami",
  "Laguna Seca": "laguna_seca",
  Misano: "misano",
  Monza: "monza",
  "Mount Panorama": "mount_panorama",
  Nurburgring: "nurburgring",
  "Oulton Park": "oulton_park",
  "Paul Ricard": "paul_ricard",
  "Red Bull Ring": "red_bull_ring",
  Silverstone: "silverstone",
  Snetterton: "snetterton",
  "Spa-Francorchamps": "spa",
  Suzuka: "suzuka",
  "Watkins Glen": "watkins_glen",
  Zandvoort: "zandvoort",
  Zolder: "zolder",
};

export function resolveTrackId(
  trackId?: string | null,
  trackName?: string | null,
): string | null {
  if (trackId?.trim()) {
    return trackId.trim().toLowerCase();
  }
  if (!trackName?.trim()) {
    return null;
  }
  const fromDisplay = DISPLAY_NAME_TO_ID[trackName.trim()];
  if (fromDisplay) {
    return fromDisplay;
  }
  return trackName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_|_$/g, "");
}

const layoutCache = new Map<string, Promise<TrackLayout | null>>();

export function loadTrackLayout(trackId: string): Promise<TrackLayout | null> {
  const id = trackId.toLowerCase();
  if (!layoutCache.has(id)) {
    layoutCache.set(
      id,
      fetch(`/tracks/${id}.json`)
        .then((res) => (res.ok ? (res.json() as Promise<TrackLayout>) : null))
        .catch(() => null),
    );
  }
  return layoutCache.get(id)!;
}

export interface PathPoint {
  x: number;
  y: number;
}

export interface PathMetrics {
  points: PathPoint[];
  cumulative: number[];
  totalLength: number;
  pointAtPct: (pct: number) => PathPoint;
  pctAtSvg: (x: number, y: number) => number;
}

export function buildPathMetrics(
  points: [number, number][],
  project: (x: number, y: number) => PathPoint,
): PathMetrics {
  const projected = points.map(([x, y]) => project(x, y));
  const cumulative = [0];
  for (let i = 1; i < projected.length; i += 1) {
    const prev = projected[i - 1];
    const curr = projected[i];
    const dx = curr.x - prev.x;
    const dy = curr.y - prev.y;
    cumulative.push(cumulative[i - 1] + Math.hypot(dx, dy));
  }
  const totalLength = cumulative[cumulative.length - 1] || 1;

  const pointAtDistance = (distance: number): PathPoint => {
    const target = Math.max(0, Math.min(distance, totalLength));
    let idx = cumulative.findIndex((d) => d >= target);
    if (idx <= 0) {
      return projected[0];
    }
    if (idx === -1) {
      return projected[projected.length - 1];
    }
    const segStart = cumulative[idx - 1];
    const segLen = cumulative[idx] - segStart || 1;
    const t = (target - segStart) / segLen;
    const a = projected[idx - 1];
    const b = projected[idx];
    return {
      x: a.x + (b.x - a.x) * t,
      y: a.y + (b.y - a.y) * t,
    };
  };

  return {
    points: projected,
    cumulative,
    totalLength,
    pointAtPct: (pct: number) =>
      pointAtDistance((Math.max(0, Math.min(pct, 100)) / 100) * totalLength),
    pctAtSvg: (x: number, y: number) => {
      let bestPct = 0;
      let bestDist = Infinity;
      for (let i = 1; i < projected.length; i += 1) {
        const a = projected[i - 1];
        const b = projected[i];
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const lenSq = dx * dx + dy * dy || 1;
        const t = Math.max(
          0,
          Math.min(1, ((x - a.x) * dx + (y - a.y) * dy) / lenSq),
        );
        const px = a.x + dx * t;
        const py = a.y + dy * t;
        const dist = Math.hypot(x - px, y - py);
        if (dist < bestDist) {
          bestDist = dist;
          const segStart = cumulative[i - 1];
          const segLen = cumulative[i] - segStart;
          bestPct = ((segStart + segLen * t) / totalLength) * 100;
        }
      }
      return bestPct;
    },
  };
}
