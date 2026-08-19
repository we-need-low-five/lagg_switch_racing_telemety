# SimTelemetry

Live telemetry recording for simracing games. Auto-detects ACC, Assetto Corsa, Le Mans Ultimate, and F1 25, records laps to local storage, highlights fastest valid laps, and provides multi-lap analysis with distance-aligned graphs, sector deltas, and track maps.

## Features

- Background auto-recording with system tray presence
- Fastest valid lap detection per session
- Manual lap pinning for reference comparisons
- Full analysis UI: speed/throttle/brake/steering/gear/RPM overlays, time delta, sector bars, track map
- Portable `.stb` session bundle export/import
- Game setup wizard with live telemetry checks



## Supported Games


| Game                       | Telemetry Source                  |
| -------------------------- | --------------------------------- |
| Assetto Corsa Competizione | Shared memory (default)           |
| Assetto Corsa              | Shared memory (default)           |
| Le Mans Ultimate           | Official `LMU_Data` shared memory |
| F1 25                      | UDP port 20777                    |




## Requirements

- Windows 10/11
- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 20+



## Development

```powershell
cd C:\Users\low_five\simtelemetry
npm install
npm run tauri dev
```



## Build Installer

```powershell
npm run tauri:build
```

Installers are produced under `src-tauri/target/release/bundle/`.

## Data Location

Sessions are stored in `%LOCALAPPDATA%\SimTelemetry\`:

- `simtelemetry.db` — session/lap metadata (SQLite)
- `sessions/{session_id}/laps/{lap_id}.parquet` — distance-aligned lap samples



## Troubleshooting

- **No telemetry detected:** open the in-app Setup Wizard and follow per-game steps.
- **F1 25:** set UDP IP to `127.0.0.1`, port `20777`, format `2025`, send rate `20 Hz`.
- **LMU:** ensure shared memory is active (LMU 1.2+) and you are on track.

See [docs/game-setup/](docs/game-setup/) for detailed setup guides.

## Architecture

- Rust workspace: game adapters, lap engine, SQLite/Parquet storage, recording daemon
- Tauri 2 desktop shell with React/TypeScript UI and uPlot charts



## License

MIT