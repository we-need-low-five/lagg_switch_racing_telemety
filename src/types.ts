export type GameId = "acc" | "ac" | "lmu" | "f1_25";

export interface SessionRecord {
  id: string;
  game: GameId;
  track_id?: string;
  track: string;
  car: string;
  started_at: string;
  ended_at?: string | null;
  game_version: string;
  player_name: string;
  lap_count: number;
  best_lap_time_ms?: number | null;
}

export interface SectorTimes {
  s1_ms?: number | null;
  s2_ms?: number | null;
  s3_ms?: number | null;
}

export interface LapRecord {
  id: string;
  session_id: string;
  lap_number: number;
  lap_time_ms: number;
  valid: boolean;
  is_best: boolean;
  is_pinned: boolean;
  sectors: SectorTimes;
  sample_rate_hz: number;
  tyre_compound?: string | null;
  tc_level?: number | null;
  abs_level?: number | null;
  fuel_used_l?: number | null;
}

export interface DistanceSample {
  distance_pct: number;
  lap_time_s: number;
  speed_mps: number;
  throttle: number;
  brake: number;
  steering: number;
  gear: number;
  rpm: number;
  pos_x: number;
  pos_y: number;
  pos_z: number;
  fuel?: number | null;
  tyre_temp_fl?: number | null;
  tyre_temp_fr?: number | null;
  tyre_temp_rl?: number | null;
  tyre_temp_rr?: number | null;
  tyre_press_fl?: number | null;
  tyre_press_fr?: number | null;
  tyre_press_rl?: number | null;
  tyre_press_rr?: number | null;
  g_force_x?: number | null;
  g_force_y?: number | null;
  g_force_z?: number | null;
  slip_angle_fl?: number | null;
  slip_angle_fr?: number | null;
  slip_angle_rl?: number | null;
  slip_angle_rr?: number | null;
}

export type DistanceSampleChannel = keyof DistanceSample;

export interface RecordingStatus {
  active: boolean;
  paused: boolean;
  game?: GameId | null;
  track?: string | null;
  current_lap: number;
  samples_recorded: number;
}

export interface GameSetupStatus {
  game: GameId;
  process_detected: boolean;
  telemetry_active: boolean;
  message: string;
}

export interface LeaderboardTrackOption {
  track_id: string;
  track: string;
}

export interface LeaderboardEntry {
  rank: number;
  player_name: string;
  lap_time_ms: number;
  valid: boolean;
  session_id: string;
  lap_id: string;
}

export interface TrackLapOption {
  lap_id: string;
  session_id: string;
  lap_number: number;
  lap_time_ms: number;
  valid: boolean;
  player_name: string;
  car: string;
  started_at: string;
  sectors: SectorTimes;
}

export function formatLapTime(ms: number): string {
  const minutes = Math.floor(ms / 60000);
  const seconds = (ms % 60000) / 1000;
  return minutes > 0
    ? `${minutes}:${seconds.toFixed(3).padStart(6, "0")}`
    : seconds.toFixed(3);
}

export function gameLabel(game: GameId): string {
  switch (game) {
    case "acc":
      return "ACC";
    case "ac":
      return "AC";
    case "lmu":
      return "LMU";
    case "f1_25":
      return "F1 25";
  }
}

const VALID_GAME_IDS: GameId[] = ["acc", "ac", "lmu", "f1_25"];

export function gameIdFromRust(value: string): GameId {
  const lower = value.toLowerCase();
  if (VALID_GAME_IDS.includes(lower as GameId)) {
    return lower as GameId;
  }
  // Legacy: Rust Debug formatting (e.g. "Acc", "F1_25")
  if (value.includes("F1")) return "f1_25";
  if (value.includes("Acc")) return "acc";
  if (value.includes("Lmu")) return "lmu";
  if (value.includes("Ac")) return "ac";
  return "acc";
}
