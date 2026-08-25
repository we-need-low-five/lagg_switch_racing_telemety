import type { SessionRecord } from "../types";
import { gameLabel } from "../types";

/** Strip characters unsafe or awkward in filenames; collapse runs of separators. */
function sanitizeFilePart(value: string, fallback: string): string {
  const cleaned = value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[<>:"/\\|?*\x00-\x1f]/g, "")
    .trim()
    .replace(/[\s.]+/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
  return cleaned.length > 0 ? cleaned : fallback;
}

function sessionDatePart(startedAt: string): string {
  const date = new Date(startedAt);
  if (Number.isNaN(date.getTime())) return "unknown-date";
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${d}-${m}-${y}`;
}

/** First initial + last name (e.g. "John Smith" → "J_Smith"). Single token kept as-is. */
function driverNamePart(playerName: string): string {
  const parts = playerName.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "Unknown";
  if (parts.length === 1) return parts[0];
  const initial = parts[0].charAt(0).toUpperCase();
  const last = parts[parts.length - 1];
  return `${initial}_${last}`;
}

/** Lap time safe for Windows filenames (`1:23.456` → `1-23-456`). */
function lapTimeFilePart(ms: number): string {
  const minutes = Math.floor(ms / 60000);
  const seconds = (ms % 60000) / 1000;
  const sec = seconds.toFixed(3).padStart(6, "0").replace(".", "-");
  return minutes > 0 ? `${minutes}-${sec}` : seconds.toFixed(3).replace(".", "-");
}

/** Suggested `.stb` filename: Game_Track_DD-MM-YYYY_I_LastName_1-23-456.stb */
export function sessionBundleFileName(session: SessionRecord): string {
  const game = sanitizeFilePart(gameLabel(session.game), "Game");
  const track = sanitizeFilePart(session.track || "Unknown_track", "Unknown_track");
  const date = sessionDatePart(session.started_at);
  const driver = sanitizeFilePart(driverNamePart(session.player_name), "Unknown");
  const best = session.best_lap_time_ms;
  const timePart =
    best != null && best > 0 ? `_${lapTimeFilePart(best)}` : "";
  return `${game}_${track}_${date}_${driver}${timePart}.stb`;
}
