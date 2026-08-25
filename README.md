# SimTelemetry

Windows desktop app for simracing telemetry. Auto-detects ACC, Assetto Corsa, Le Mans Ultimate, and F1 25, records laps in the background (system tray), and provides session review plus multi-lap analysis with distance-aligned charts, segment deltas, and track maps.

## Features

- Background auto-recording with system tray presence and pause/resume
- Session browser with **Review session** entry, export/delete overflow menu, and `.stb` import
- Session review: lap table with game sector times (S1–S3), delta vs session best, and ACC extras (tyre compound, TC/ABS, fuel used)
- Lap analysis: speed / throttle / brake / steering / gear / RPM overlays, time delta, fuel & tyre temp/pressure charts (ACC when recorded)
- Ten equal-distance segment deltas with click-to-zoom (analysis); game sectors kept on the review table
- Linked cursor across charts (`distance_pct`) and track map (bundled ACC layouts)
- Cross-session **Compare** and **Leaderboard** by game/track
- Manual **Fuel** calculator for race planning
- Settings: game setup checks, display units, appearance (background presets / custom, graph lap colors)
- Portable `.stb` session bundles (v2; v1 still imports)

## Supported Games

| Game | Telemetry source | Notes |
|------|------------------|--------|
| Assetto Corsa Competizione | Shared memory (`acpmf_*`) | Reference implementation — fullest capture & analysis |
| Assetto Corsa | Shared memory | Basic adapter |
| Le Mans Ultimate | Official `LMU_Data` shared memory | Basic adapter (LMU 1.2+) |
| F1 25 | UDP `127.0.0.1:20777` | Basic adapter |

Track map outlines are bundled for ACC circuits under `public/tracks/`. Other games record and analyze; map coverage depends on matching track ids.

## Requirements

- Windows 10/11
- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 20+

## Development

```powershell
npm install
npm run tauri dev
```

Frontend-only (no Rust / tray / IPC):

```powershell
npm run dev
```

Workspace Rust checks:

```powershell
cargo check
cargo test
```

Regenerate track layouts (network):

```powershell
npm run build:tracks
```

## Build Installer

```powershell
npm run tauri:build
```

Installers are produced under `src-tauri/target/release/bundle/` (MSI and NSIS). See [installer/README.md](installer/README.md).

## App Routes

| Route | Purpose |
|-------|---------|
| `/` | Sessions |
| `/sessions/:sessionId` | Session review |
| `/compare` | Cross-session compare by track |
| `/compare/:sessionId` | In-session lap analysis |
| `/leaderboard` | Best laps by game/track |
| `/fuel` | Fuel calculator |
| `/settings` | Game setup, units, appearance |

## Data Location

Runtime data lives in `%LOCALAPPDATA%\SimTelemetry\`:

- `simtelemetry.db` — session/lap metadata and persistent leaderboard personal bests (SQLite)
- `sessions/{session_id}/laps/{lap_id}.parquet` — distance-aligned lap samples
- `leaderboard/laps/{id}.parquet` — copies of each driver's top 3 laps per track (kept if the source session is deleted)
- `logs/` — application logs

Portable export format: `.stb` (ZIP). See [docs/bundle-format.md](docs/bundle-format.md).

## Troubleshooting

- **No telemetry detected:** open **Settings → Game Setup** and follow the live checks for your game.
- **F1 25:** UDP IP `127.0.0.1`, port `20777`, format `2025`, send rate `20 Hz`.
- **LMU:** shared memory active (LMU 1.2+) and car on track.
- **ACC extras missing on old laps:** tyre/fuel channels and compound/TC/ABS are recorded for new ACC laps only; older Parquet files stay readable without those columns.

Per-game guides: [docs/game-setup/](docs/game-setup/).

## Architecture

- **Rust workspace** (`crates/*`): game adapters, lap engine, SQLite/Parquet storage, recording daemon
- **Tauri 2** desktop shell (`src-tauri`) with React 19 / TypeScript / Vite UI and uPlot charts
- Frontend talks to the backend only through `src/api.ts`

More detail for contributors: [AGENTS.md](AGENTS.md) and [docs/implementation-plan.md](docs/implementation-plan.md).

## License

MIT
