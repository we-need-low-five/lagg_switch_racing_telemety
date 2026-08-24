mod commands;
mod state;

use parking_lot::Mutex;
use sim_daemon::RecordingService;
use sim_storage::{default_data_dir, Database};
use state::AppState;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

/// Recorder poll interval. 3 ms matches ACC physics (~333 Hz) without spinning.
const RECORDER_POLL_INTERVAL: Duration = Duration::from_millis(3);

#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
}

/// Windows `Sleep` defaults to ~15.6 ms unless the process requests a finer period.
fn enable_high_resolution_timers() {
    #[cfg(windows)]
    // SAFETY: Requests 1 ms multimedia timer resolution for this process.
    unsafe {
        let _ = timeBeginPeriod(1);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    let data_dir = default_data_dir();
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(data_dir.join("sessions")).ok();
    std::fs::create_dir_all(data_dir.join("logs")).ok();

    let db = Arc::new(Mutex::new(
        Database::open(&data_dir.join("simtelemetry.db")).expect("open database"),
    ));
    let recorder = RecordingService::new(db.clone(), data_dir.clone());
    let state = Arc::new(AppState::new(db, recorder, data_dir));

    let recorder_state = state.clone();
    thread::spawn(move || {
        enable_high_resolution_timers();
        loop {
            // Only the recorder mutex is needed for polling; DB is locked briefly inside flush.
            if let Some(mut guard) = recorder_state.recorder.try_lock() {
                if let Ok(Some(msg)) = guard.tick() {
                    recorder_state.set_last_notification(msg);
                }
            }
            thread::sleep(RECORDER_POLL_INTERVAL);
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state.clone())
        .setup(move |app| {
            let show_item = MenuItem::with_id(app, "show", "Open SimTelemetry", true, None::<&str>)?;
            let pause_item = MenuItem::with_id(app, "pause", "Pause Recording", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &pause_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("SimTelemetry — waiting for game")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "pause" => {
                        if let Some(state) = app.try_state::<Arc<AppState>>() {
                            let mut guard = state.recorder.lock();
                            let paused = !guard.is_paused();
                            guard.set_paused(paused);
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::get_session,
            commands::list_laps,
            commands::load_lap_samples,
            commands::get_recording_status,
            commands::set_recording_paused,
            commands::pin_lap,
            commands::delete_session,
            commands::export_session_bundle,
            commands::import_session_bundle,
            commands::check_game_setup,
            commands::get_data_dir,
            commands::list_leaderboard_games,
            commands::list_leaderboard_tracks,
            commands::get_leaderboard,
            commands::list_track_laps,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
