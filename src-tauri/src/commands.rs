use crate::state::AppState;
use sim_core::{
    DistanceSample, FuelProfile, GameId, GameSetupStatus, LapRecord, LeaderboardEntry,
    LeaderboardTrackOption, RecordingStatus, SessionRecord, TrackLapOption,
};
use sim_daemon::GameSetupProbe;
use sim_storage::{
    export_session_bundle as storage_export_session_bundle,
    import_session_bundle as storage_import_session_bundle, read_lap_samples,
    resolve_data_relative, validate_bundle_path,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn list_sessions(state: State<'_, Arc<AppState>>) -> Result<Vec<SessionRecord>, String> {
    state.db.lock().list_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<Option<SessionRecord>, String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state
        .db
        .lock()
        .get_session(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_laps(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<Vec<LapRecord>, String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state.db.lock().list_laps(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_lap_samples(
    state: State<'_, Arc<AppState>>,
    lap_id: String,
) -> Result<Vec<DistanceSample>, String> {
    let id = Uuid::parse_str(&lap_id).map_err(|e| e.to_string())?;
    let abs = {
        let path = state
            .db
            .lock()
            .get_lap_parquet_path(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "lap file not found".to_string())?;
        resolve_data_relative(state.data_dir(), &path).map_err(|e| e.to_string())?
    };
    // File I/O outside the DB lock so the recorder can keep flushing.
    read_lap_samples(&abs).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recording_status(state: State<'_, Arc<AppState>>) -> Result<RecordingStatus, String> {
    Ok(state.recorder.lock().status())
}

#[tauri::command]
pub fn set_recording_paused(
    state: State<'_, Arc<AppState>>,
    paused: bool,
) -> Result<(), String> {
    state.recorder.lock().set_paused(paused);
    Ok(())
}

#[tauri::command]
pub fn pin_lap(
    state: State<'_, Arc<AppState>>,
    lap_id: String,
    pinned: bool,
) -> Result<(), String> {
    let id = Uuid::parse_str(&lap_id).map_err(|e| e.to_string())?;
    state
        .db
        .lock()
        .set_lap_pinned(id, pinned)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_session(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state
        .db
        .lock()
        .delete_session(id, state.data_dir())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_session_bundle(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    output_path: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let output = PathBuf::from(&output_path);
    validate_bundle_path(&output).map_err(|e| e.to_string())?;
    let data_dir = state.data_dir().clone();
    // Hold DB only for export; zip I/O runs inside storage while we hold the lock
    // briefly relative to the old global AppState mutex that also blocked the recorder.
    storage_export_session_bundle(&state.db.lock(), &data_dir, id, &output)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_session_bundle(
    state: State<'_, Arc<AppState>>,
    bundle_path: String,
) -> Result<String, String> {
    let path = PathBuf::from(&bundle_path);
    validate_bundle_path(&path).map_err(|e| e.to_string())?;
    let data_dir = state.data_dir().clone();
    let id = storage_import_session_bundle(&state.db.lock(), &data_dir, &path)
        .map_err(|e| e.to_string())?;
    Ok(id.to_string())
}

#[tauri::command]
pub fn check_game_setup(game: String) -> Result<GameSetupStatus, String> {
    let game_id = parse_game_id(&game)?;
    Ok(GameSetupProbe::check(game_id))
}

#[tauri::command]
pub fn get_data_dir(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state.data_dir().display().to_string())
}

#[tauri::command]
pub fn list_leaderboard_games(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<GameId>, String> {
    state
        .db
        .lock()
        .list_leaderboard_games()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_leaderboard_tracks(
    state: State<'_, Arc<AppState>>,
    game: String,
) -> Result<Vec<LeaderboardTrackOption>, String> {
    let game_id = parse_game_id(&game)?;
    state
        .db
        .lock()
        .list_leaderboard_tracks(game_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_leaderboard(
    state: State<'_, Arc<AppState>>,
    game: String,
    track_id: String,
    track_name: String,
) -> Result<Vec<LeaderboardEntry>, String> {
    let game_id = parse_game_id(&game)?;
    state
        .db
        .lock()
        .get_leaderboard(game_id, &track_id, &track_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_track_laps(
    state: State<'_, Arc<AppState>>,
    game: String,
    track_id: String,
    track_name: String,
) -> Result<Vec<TrackLapOption>, String> {
    let game_id = parse_game_id(&game)?;
    state
        .db
        .lock()
        .list_track_laps(game_id, &track_id, &track_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_fuel_profiles(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<FuelProfile>, String> {
    state
        .db
        .lock()
        .list_fuel_profiles()
        .map_err(|e| e.to_string())
}

fn parse_game_id(value: &str) -> Result<GameId, String> {
    match value {
        "acc" => Ok(GameId::Acc),
        "ac" => Ok(GameId::Ac),
        "lmu" => Ok(GameId::Lmu),
        "f1_25" => Ok(GameId::F1_25),
        _ => Err(format!("unknown game: {value}")),
    }
}
