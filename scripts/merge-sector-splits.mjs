/**
 * Merges ACC_SECTOR_SPLITS into public/tracks/*.json as sector_splits field.
 * Run: node scripts/merge-sector-splits.mjs
 */
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const tracksDir = path.join(__dirname, "..", "public", "tracks");

const SPLITS = {
  monza: [33.8, 67.2],
  spa: [32.5, 65.8],
  brands_hatch: [35.2, 68.5],
  silverstone: [34.1, 67.0],
  barcelona: [33.5, 66.8],
  hungaroring: [34.8, 68.2],
  zandvoort: [33.2, 66.5],
  cota: [34.5, 67.5],
  indianapolis: [35.0, 68.0],
  suzuka: [33.0, 66.2],
  nurburgring: [34.2, 67.1],
  misano: [34.6, 67.4],
  imola: [33.7, 66.9],
  kyalami: [34.0, 67.2],
  mount_panorama: [35.5, 69.0],
  laguna_seca: [33.4, 66.6],
  watkins_glen: [34.3, 67.3],
  donington: [34.8, 68.1],
  oulton_park: [35.1, 68.4],
  snetterton: [34.5, 67.6],
  paul_ricard: [33.9, 67.0],
  zolder: [34.2, 67.5],
};

for (const [id, splits] of Object.entries(SPLITS)) {
  const filePath = path.join(tracksDir, `${id}.json`);
  try {
    const layout = JSON.parse(await readFile(filePath, "utf8"));
    layout.sector_splits = splits;
    await writeFile(filePath, `${JSON.stringify(layout)}\n`);
    console.log(`updated ${id}`);
  } catch {
    console.warn(`skip ${id}`);
  }
}
