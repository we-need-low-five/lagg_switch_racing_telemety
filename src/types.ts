export type GameId = "acc" | "ac" | "lmu" | "f1_25";

export type SessionKind =
  | "unknown"
  | "practice"
  | "qualifying"
  | "race"
  | "hotlap"
  | "time_attack"
  | "other";

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
  /** Phase the session began as, when the sim reports it. */
  session_kind?: SessionKind;
  /** Every distinct phase the session's stints cover, in run order. */
  session_kinds?: SessionKind[];
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
  /** 1-based stint; omitted/legacy laps are treated as stint 1. */
  stint?: number;
  /** Seconds of frozen physics that opened this stint; only on the first lap of stints 2+. */
  stint_break_s?: number | null;
  /** Weekend phase this stint belongs to, when the sim reports one. */
  stint_kind?: SessionKind | null;
  /**
   * Metres the car drove over the lap, measured from its recorded positions.
   * Separates a full lap from one that reached the same start/finish line by a
   * shorter route — a joker lap round the Nurburgring 24h GP loop. Null when
   * the trace held no usable positions.
   */
  lap_distance_m?: number | null;
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
  place?: number;
  player_name: string;
  /** Kept on the leaderboard row, so it outlives the source session. */
  car: string;
  lap_time_ms: number;
  valid: boolean;
  session_id: string;
  lap_id: string;
  session_exists?: boolean;
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

export interface FuelProfile {
  game: GameId;
  car: string;
  track: string;
  avg_lap_time_ms?: number | null;
  avg_fuel_used_l?: number | null;
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

/** Display label for a session type, or null when the sim never reported one. */
export function sessionKindLabel(kind: SessionKind | undefined): string | null {
  switch (kind) {
    case "practice":
      return "Practice";
    case "qualifying":
      return "Qualifying";
    case "race":
      return "Race";
    case "hotlap":
      return "Hotlap";
    case "time_attack":
      return "Time Attack";
    default:
      return null;
  }
}

/** One label per phase a session covers (in run order), for the session card. */
export function sessionPhaseLabels(session: {
  session_kinds?: SessionKind[] | null;
  session_kind?: SessionKind | null;
}): string[] {
  const kinds =
    session.session_kinds && session.session_kinds.length > 0
      ? session.session_kinds
      : session.session_kind
        ? [session.session_kind]
        : [];
  return kinds
    .map((k) => sessionKindLabel(k ?? undefined))
    .filter((l): l is string => l != null);
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
