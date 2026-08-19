use sim_capture_acc::acc_telemetry_available;
use sim_capture_ac::AcAdapter;
use sim_capture_f1::F1Adapter;
use sim_capture_lmu::LmuAdapter;
use sim_core::{GameAdapter, GameId, GameSetupStatus};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

pub struct GameSetupProbe;

impl GameSetupProbe {
    pub fn check(game: GameId) -> GameSetupStatus {
        let telemetry_active = telemetry_available(game);
        let process_detected = process_running(game) || telemetry_active;

        let message = if telemetry_active {
            "Telemetry detected".to_string()
        } else if process_running(game) {
            setup_hint(game)
        } else {
            format!("Launch {} and enter a session", game.label())
        };

        GameSetupStatus {
            game,
            process_detected,
            telemetry_active,
            message,
        }
    }
}

pub fn detect_running_game() -> Option<GameId> {
    if acc_telemetry_available() {
        return Some(GameId::Acc);
    }

    for game in [GameId::Ac, GameId::Lmu, GameId::F1_25] {
        if telemetry_available(game) || process_running(game) {
            return Some(game);
        }
    }

    None
}

fn telemetry_available(game: GameId) -> bool {
    match game {
        GameId::Acc => acc_telemetry_available(),
        GameId::Ac => {
            let mut adapter = AcAdapter::new();
            !matches!(adapter.poll(), sim_core::AdapterEvent::Disconnected)
        }
        GameId::Lmu => {
            let mut adapter = LmuAdapter::new();
            !matches!(adapter.poll(), sim_core::AdapterEvent::Disconnected)
        }
        GameId::F1_25 => {
            let mut adapter = F1Adapter::new();
            let _ = adapter.poll();
            adapter.is_active()
        }
    }
}

fn process_running(game: GameId) -> bool {
    let mut system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    );
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let names = process_names(game);
    system.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        names.iter().any(|n| name.contains(n))
    })
}

fn process_names(game: GameId) -> &'static [&'static str] {
    match game {
        GameId::Acc => &[
            "ac2-win64-shipping",
            "assetto corsa competizione",
            "assettocorsacompetizione",
            "acc.exe",
        ],
        GameId::Ac => &["acs.exe", "assetto corsa"],
        GameId::Lmu => &["le mans ultimate", "lmu.exe"],
        GameId::F1_25 => &["f1_25.exe", "f1 25"],
    }
}

fn setup_hint(game: GameId) -> String {
    match game {
        GameId::Acc | GameId::Ac => {
            "Game running but no telemetry yet — enter a driving session (not just menus)".into()
        }
        GameId::Lmu => {
            "Ensure LMU shared memory is enabled (LMU 1.2+) and you are on track".into()
        }
        GameId::F1_25 => {
            "Enable UDP telemetry in game settings: IP 127.0.0.1, port 20777, format 2025".into()
        }
    }
}
