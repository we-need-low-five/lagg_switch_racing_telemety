import { invoke } from "@tauri-apps/api/core";
import type {
  DistanceSample,
  GameId,
  GameSetupStatus,
  LapRecord,
  LeaderboardEntry,
  LeaderboardTrackOption,
  RecordingStatus,
  SessionRecord,
  TrackLapOption,
  FuelProfile,
} from "./types";
import { gameIdFromRust } from "./types";

function normalizeSession(raw: SessionRecord): SessionRecord {
  return {
    ...raw,
    game: typeof raw.game === "string" ? gameIdFromRust(raw.game) : raw.game,
  };
}

export async function listSessions(): Promise<SessionRecord[]> {
  const rows = await invoke<SessionRecord[]>("list_sessions");
  return rows.map(normalizeSession);
}

export async function getSession(sessionId: string): Promise<SessionRecord | null> {
  const row = await invoke<SessionRecord | null>("get_session", { sessionId });
  return row ? normalizeSession(row) : null;
}

export async function listLaps(sessionId: string): Promise<LapRecord[]> {
  return invoke<LapRecord[]>("list_laps", { sessionId });
}

export async function loadLapSamples(lapId: string): Promise<DistanceSample[]> {
  return invoke<DistanceSample[]>("load_lap_samples", { lapId });
}

export async function getRecordingStatus(): Promise<RecordingStatus> {
  const status = await invoke<RecordingStatus>("get_recording_status");
  return {
    ...status,
    game: status.game
      ? typeof status.game === "string"
        ? gameIdFromRust(status.game)
        : status.game
      : null,
  };
}

export async function setRecordingPaused(paused: boolean): Promise<void> {
  await invoke("set_recording_paused", { paused });
}

export async function pinLap(lapId: string, pinned: boolean): Promise<void> {
  await invoke("pin_lap", { lapId, pinned });
}

export async function deleteSession(sessionId: string): Promise<void> {
  await invoke("delete_session", { sessionId });
}

export async function exportSessionBundle(
  sessionId: string,
  outputPath: string,
): Promise<void> {
  await invoke("export_session_bundle", { sessionId, outputPath });
}

export async function importSessionBundle(bundlePath: string): Promise<string> {
  return invoke<string>("import_session_bundle", { bundlePath });
}

export async function checkGameSetup(game: GameId): Promise<GameSetupStatus> {
  const status = await invoke<GameSetupStatus>("check_game_setup", { game });
  return {
    ...status,
    game: typeof status.game === "string" ? gameIdFromRust(status.game) : status.game,
  };
}

export async function getDataDir(): Promise<string> {
  return invoke<string>("get_data_dir");
}

export async function listLeaderboardGames(): Promise<GameId[]> {
  const rows = await invoke<GameId[] | string[]>("list_leaderboard_games");
  return rows.map((g) => (typeof g === "string" ? gameIdFromRust(g) : g));
}

export async function listLeaderboardTracks(
  game: GameId,
): Promise<LeaderboardTrackOption[]> {
  return invoke<LeaderboardTrackOption[]>("list_leaderboard_tracks", { game });
}

export async function getLeaderboard(
  game: GameId,
  trackId: string,
  trackName: string,
): Promise<LeaderboardEntry[]> {
  return invoke<LeaderboardEntry[]>("get_leaderboard", {
    game,
    trackId,
    trackName,
  });
}

export async function listFuelProfiles(): Promise<FuelProfile[]> {
  const rows = await invoke<FuelProfile[]>("list_fuel_profiles");
  return rows.map((row) => ({
    ...row,
    game: typeof row.game === "string" ? gameIdFromRust(row.game) : row.game,
  }));
}

export async function listTrackLaps(
  game: GameId,
  trackId: string,
  trackName: string,
): Promise<TrackLapOption[]> {
  return invoke<TrackLapOption[]>("list_track_laps", {
    game,
    trackId,
    trackName,
  });
}
