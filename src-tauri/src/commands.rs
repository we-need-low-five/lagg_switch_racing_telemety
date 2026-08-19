use crate::state::AppState;
use parking_lot::Mutex;
use sim_core::{
    DistanceSample, GameId, GameSetupStatus, LapRecord, LeaderboardEntry,
    LeaderboardTrackOption, RecordingStatus, SessionRecord, TrackLapOption,
};
use sim_daemon::GameSetupProbe;
use sim_storage::{
    export_session_bundle as storage_export_session_bundle,
    import_session_bundle as storage_import_session_bundle,
    read_lap_samples,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn list_sessions(state: State<'_, Arc<Mutex<AppState>>>) -> Result<Vec<SessionRecord>, String> {
    state.lock().db().list_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session(
    state: State<'_, Arc<Mutex<AppState>>>,
    session_id: String,
) -> Result<Option<SessionRecord>, String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state
        .lock()
        .db()
        .get_session(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_laps(state: State<'_, Arc<Mutex<AppState>>>, session_id: String) -> Result<Vec<LapRecord>, String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state.lock().db().list_laps(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_lap_samples(state: State<'_, Arc<Mutex<AppState>>>, lap_id: String) -> Result<Vec<DistanceSample>, String> {
    let id = Uuid::parse_str(&lap_id).map_err(|e| e.to_string())?;
    let guard = state.lock();
    let path = guard
        .db()
        .get_lap_parquet_path(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "lap file not found".to_string())?;
    let abs = guard.data_dir().join(path);
    read_lap_samples(&abs).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recording_status(state: State<'_, Arc<Mutex<AppState>>>) -> Result<RecordingStatus, String> {
    Ok(state.lock().recorder().status())
}

#[tauri::command]
pub fn set_recording_paused(state: State<'_, Arc<Mutex<AppState>>>, paused: bool) -> Result<(), String> {
    state.lock().recorder_mut().set_paused(paused);
    Ok(())
}

#[tauri::command]
pub fn pin_lap(state: State<'_, Arc<Mutex<AppState>>>, lap_id: String, pinned: bool) -> Result<(), String> {
    let id = Uuid::parse_str(&lap_id).map_err(|e| e.to_string())?;
    state.lock().recorder().pin_lap(id, pinned).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_session(state: State<'_, Arc<Mutex<AppState>>>, session_id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    state.lock().db().delete_session(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_session_bundle(
    state: State<'_, Arc<Mutex<AppState>>>,
    session_id: String,
    output_path: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let guard = state.lock();
    storage_export_session_bundle(
        guard.db(),
        guard.data_dir(),
        id,
        &PathBuf::from(output_path),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_session_bundle(
    state: State<'_, Arc<Mutex<AppState>>>,
    bundle_path: String,
) -> Result<String, String> {
    let guard = state.lock();
    let id = storage_import_session_bundle(
        guard.db(),
        guard.data_dir(),
        &PathBuf::from(bundle_path),
    )
    .map_err(|e| e.to_string())?;
    Ok(id.to_string())
}

#[tauri::command]
pub fn check_game_setup(game: String) -> Result<GameSetupStatus, String> {
    let game_id = parse_game_id(&game)?;
    Ok(GameSetupProbe::check(game_id))
}

#[tauri::command]
pub fn get_data_dir(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, String> {
    Ok(state.lock().data_dir().display().to_string())
}

#[tauri::command]
pub fn list_leaderboard_games(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<GameId>, String> {
    state
        .lock()
        .db()
        .list_leaderboard_games()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_leaderboard_tracks(
    state: State<'_, Arc<Mutex<AppState>>>,
    game: String,
) -> Result<Vec<LeaderboardTrackOption>, String> {
    let game_id = parse_game_id(&game)?;
    state
        .lock()
        .db()
        .list_leaderboard_tracks(game_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_leaderboard(
    state: State<'_, Arc<Mutex<AppState>>>,
    game: String,
    track_id: String,
    track_name: String,
) -> Result<Vec<LeaderboardEntry>, String> {
    let game_id = parse_game_id(&game)?;
    state
        .lock()
        .db()
        .get_leaderboard(game_id, &track_id, &track_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_track_laps(
    state: State<'_, Arc<Mutex<AppState>>>,
    game: String,
    track_id: String,
    track_name: String,
) -> Result<Vec<TrackLapOption>, String> {
    let game_id = parse_game_id(&game)?;
    state
        .lock()
        .db()
        .list_track_laps(game_id, &track_id, &track_name)
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
