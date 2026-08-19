import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(__dirname, "..", "public", "tracks");

/** ACC internal track id -> layout sources (first match wins) */
const ACC_TRACKS = {
  monza: { name: "Monza", tum: "Monza" },
  spa: { name: "Spa", tum: "Spa" },
  brands_hatch: { name: "Brands Hatch", tum: "BrandsHatch" },
  silverstone: { name: "Silverstone", tum: "Silverstone" },
  barcelona: { name: "Barcelona", tum: "Catalunya" },
  hungaroring: { name: "Hungaroring", tum: "Budapest" },
  zandvoort: { name: "Zandvoort", tum: "Zandvoort" },
  cota: { name: "Circuit of the Americas", tum: "Austin" },
  indianapolis: { name: "Indianapolis", rtapi: 36 },
  suzuka: { name: "Suzuka", tum: "Suzuka" },
  nurburgring: { name: "Nurburgring", tum: "Nuerburgring" },
  misano: { name: "Misano", rtapi: 95 },
  imola: { name: "Imola", geojson: "it-1953.geojson" },
  kyalami: { name: "Kyalami", geojson: "za-1961.geojson" },
  mount_panorama: { name: "Mount Panorama", rtapi: 179 },
  laguna_seca: { name: "Laguna Seca", rtapi: 30 },
  watkins_glen: { name: "Watkins Glen", rtapi: 32 },
  donington: { name: "Donington", rtapi: 85 },
  oulton_park: { name: "Oulton Park", rtapi: 198 },
  snetterton: { name: "Snetterton", rtapi: 200 },
  paul_ricard: { name: "Paul Ricard", rtapi: 96 },
  red_bull_ring: { name: "Red Bull Ring", tum: "Spielberg", rtapi: 14 },
  zolder: { name: "Zolder", rtapi: 84 },
};

const TARGET_POINTS = 600;
const F1_BASE =
  "https://raw.githubusercontent.com/bacinger/f1-circuits/master/circuits";
const RTAPI_TRACKS =
  "https://ligasavbrasil.github.io/RaceTracksAPI/tracks.json";
const RTAPI_IMAGES =
  "https://ligasavbrasil.github.io/RaceTracksAPI/images";

let rtapiCatalog = null;

async function fetchCsv(name) {
  const url = `https://raw.githubusercontent.com/TUMFTM/racetrack-database/master/tracks/${encodeURIComponent(name)}.csv`;
  const res = await fetch(url);
  if (!res.ok) {
    return null;
  }
  return res.text();
}

async function fetchGeoJson(file) {
  const res = await fetch(`${F1_BASE}/${file}`);
  if (!res.ok) {
    return null;
  }
  return res.json();
}

async function fetchRtapiCatalog() {
  if (!rtapiCatalog) {
    const res = await fetch(RTAPI_TRACKS);
    rtapiCatalog = res.ok ? await res.json() : [];
  }
  return rtapiCatalog;
}

async function fetchRtapiOutline(trackId) {
  const catalog = await fetchRtapiCatalog();
  const track = catalog.find((entry) => entry.id === trackId);
  const imageId = track?.layouts?.[0]?.image_id;
  if (!imageId) {
    return null;
  }
  const res = await fetch(`${RTAPI_IMAGES}/track_${imageId}.svg`);
  if (!res.ok) {
    return null;
  }
  return parseSvgPolyline(await res.text());
}

function parseSvgPolyline(svg) {
  const match = svg.match(/<polyline[^>]*points="([^"]+)"/i);
  if (!match) {
    return [];
  }
  const points = [];
  for (const token of match[1].trim().split(/\s+/)) {
    const [x, y] = token.split(",").map((v) => Number.parseFloat(v));
    if (Number.isFinite(x) && Number.isFinite(y)) {
      points.push([x, y]);
    }
  }
  return points;
}

function parseCsv(text) {
  const lines = text.trim().split("\n");
  const points = [];
  for (const line of lines.slice(1)) {
    const [x, y] = line.split(",").map((v) => Number.parseFloat(v.trim()));
    if (Number.isFinite(x) && Number.isFinite(y)) {
      points.push([x, y]);
    }
  }
  return points;
}

function parseGeoJson(geojson) {
  const geom = geojson?.features?.[0]?.geometry ?? geojson?.geometry;
  if (!geom) {
    return [];
  }
  if (geom.type === "LineString") {
    return geom.coordinates.map(([lon, lat]) => [lon, lat]);
  }
  if (geom.type === "MultiLineString") {
    return geom.coordinates.flat().map(([lon, lat]) => [lon, lat]);
  }
  if (geom.type === "Polygon") {
    return geom.coordinates[0].map(([lon, lat]) => [lon, lat]);
  }
  return [];
}

function lonLatToMeters(points) {
  if (points.length === 0) {
    return [];
  }
  const refLat =
    points.reduce((sum, [, lat]) => sum + lat, 0) / points.length;
  const refLon =
    points.reduce((sum, [lon]) => sum + lon, 0) / points.length;
  const latRad = (refLat * Math.PI) / 180;
  const mPerDegLat = 111_320;
  const mPerDegLon = 111_320 * Math.cos(latRad);
  return points.map(([lon, lat]) => [
    (lon - refLon) * mPerDegLon,
    (lat - refLat) * mPerDegLat,
  ]);
}

function downsample(points, target) {
  if (points.length <= target) {
    return points;
  }
  const result = [];
  for (let i = 0; i < target; i += 1) {
    const idx = Math.round((i / (target - 1)) * (points.length - 1));
    result.push(points[idx]);
  }
  return result;
}

function normalize(points) {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const spanX = maxX - minX || 1;
  const spanY = maxY - minY || 1;
  const scale = Math.max(spanX, spanY);
  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  return points.map(([x, y]) => [
    Number((((x - cx) / scale) * 2).toFixed(5)),
    Number((((y - cy) / scale) * 2).toFixed(5)),
  ]);
}

async function loadTrackPoints(trackId, config) {
  if (config.tum) {
    const csv = await fetchCsv(config.tum);
    if (csv) {
      const raw = parseCsv(csv);
      if (raw.length >= 10) {
        return { source: "tum", points: raw };
      }
    }
  }
  if (config.geojson) {
    const geojson = await fetchGeoJson(config.geojson);
    const raw = lonLatToMeters(parseGeoJson(geojson ?? {}));
    if (raw.length >= 10) {
      return { source: "geojson", points: raw };
    }
  }
  if (config.rtapi) {
    const raw = (await fetchRtapiOutline(config.rtapi)) ?? [];
    if (raw.length >= 10) {
      return { source: "rtapi", points: raw };
    }
  }
  return null;
}

await mkdir(outDir, { recursive: true });

const index = {};
const missing = [];

for (const [trackId, config] of Object.entries(ACC_TRACKS)) {
  const loaded = await loadTrackPoints(trackId, config);
  if (!loaded) {
    missing.push(trackId);
    continue;
  }
  const points = normalize(downsample(loaded.points, TARGET_POINTS));
  const layout = { id: trackId, name: config.name, points };
  await writeFile(
    path.join(outDir, `${trackId}.json`),
    `${JSON.stringify(layout)}\n`,
  );
  index[trackId] = config.name;
  console.log(`built ${trackId} (${points.length} points, ${loaded.source})`);
}

await writeFile(
  path.join(outDir, "index.json"),
  `${JSON.stringify(index, null, 2)}\n`,
);

if (missing.length > 0) {
  console.warn("Missing layouts:", missing);
}
