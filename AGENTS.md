# AGENTS.md — SimTelemetry

Guidance for AI agents and contributors working in this repository.

## Project overview

**SimTelemetry** is a Windows desktop app that auto-detects simracing games, records lap telemetry in the background (system tray), and provides multi-lap analysis (distance-aligned charts, sector deltas, track maps).

| Layer | Stack |
|-------|--------|
| Desktop shell | Tauri 2 (`src-tauri`) |
| Backend | Rust workspace (`crates/*`) |
| Frontend | React 19 + TypeScript + Vite 7 + uPlot |
| Storage | SQLite (`rusqlite`) + Parquet lap samples |
| Platform | Windows 10/11 only (shared memory / UDP capture) |

**Supported games**

| Game | Adapter crate | Source |
|------|---------------|--------|
| Assetto Corsa Competizione | `sim-capture-acc` | Shared memory (`acpmf_*`) — reference implementation |
| Assetto Corsa | `sim-capture-ac` | Shared memory |
| Le Mans Ultimate | `sim-capture-lmu` | `LMU_Data` shared memory |
| F1 25 | `sim-capture-f1` | UDP `127.0.0.1:20777` |

Runtime data lives in `%LOCALAPPDATA%\SimTelemetry\` (`simtelemetry.db`, `sessions/{id}/laps/*.parquet`, `logs/`).

---

## Repository layout

```
simtelemetry/
├── src/                    # React/TS frontend
│   ├── api.ts             # Tauri invoke wrappers (only IPC entry from UI)
│   ├── types.ts           # UI types mirrored from Rust schema
│   ├── App.tsx / App.css   # Shell + routes
│   ├── views/             # Route pages (Sessions, LapCompare, …)
│   ├── components/        # UI + charts/ + compare/
│   ├── lib/               # Pure TS helpers (fuel, sectors, chart align, …)
│   └── assets/
├── src-tauri/             # Tauri app crate (`simtelemetry`)
│   ├── src/               # lib.rs, commands.rs, state.rs, main.rs
│   ├── capabilities/     # Tauri permission capabilities
│   └── tauri.conf.json
├── crates/
│   ├── core/              # sim-core — schema, GameAdapter trait, lap/resample
│   ├── storage/           # sim-storage — SQLite, Parquet, .stb bundles
│   ├── capture-common/    # Shared-memory helpers (Windows)
│   ├── capture-acc|ac|lmu|f1/  # Per-game adapters
│   └── daemon/            # Game detector + RecordingService
├── scripts/               # Node utilities (track layouts, sector merges)
├── public/tracks/         # Predetermined track outline JSON
├── docs/                  # Setup guides, bundle format, implementation plan
├── installer/             # Installer notes
├── Cargo.toml             # Workspace root
└── package.json           # Frontend + Tauri npm scripts
```

Workspace members are listed in root `Cargo.toml`. Do not add crates outside that list without updating the workspace.

---

## Requirements

- Windows 10/11
- Rust **1.77+** (edition 2021)
- Node.js **20+**
- npm (lockfile: `package-lock.json`)

---

## Build, run, and test commands

### Frontend / Tauri (primary workflow)

```powershell
npm install
npm run tauri dev          # preferred: Vite :1420 + Rust backend
npm run tauri:build        # release + MSI/NSIS under src-tauri/target/release/bundle/
```

Frontend-only (no Rust/tray):

```powershell
npm run dev                # Vite only — Tauri IPC will not work
npm run build              # tsc && vite build → dist/
npm run preview            # preview production frontend
npm run build:tracks       # regenerate public/tracks/*.json (network)
```

`tauri.conf.json` hooks:

- `beforeDevCommand` → `npm run dev`
- `beforeBuildCommand` → `npm run build`
- `devUrl` → `http://localhost:1420` (strict port)

### Rust workspace

```powershell
cargo check                # whole workspace
cargo build -p simtelemetry
cargo test                 # all workspace unit tests
cargo test -p sim-core
cargo test -p sim-storage
cargo test -p sim-capture-acc
```

There is **no** ESLint/Prettier/clippy CI config in-repo. Rely on:

- TypeScript: `tsc` via `npm run build` (`strict`, unused locals/params)
- Rust: `cargo test` / `cargo check`

### Useful scripts

| Script | Purpose |
|--------|---------|
| `scripts/build-tracks.mjs` | Fetch/normalize track centerlines → `public/tracks/` |
| `scripts/merge-sector-splits.mjs` | One-off sector split merge helper |

---

## Architecture notes (do not violate)

### Dependency direction

```
UI (src/) → Tauri commands (src-tauri) → daemon/storage/core
capture-* → core (+ capture-common where needed)
daemon → core + storage + all capture-*
```

- Put shared domain types in **`sim-core`** (`schema`, `GameAdapter`, resampling).
- Put persistence and `.stb` I/O in **`sim-storage`**.
- Put game-specific packet/mapping logic only in the matching **`capture-*`** crate.
- Frontend must call backend only through **`src/api.ts`** (`invoke`), not ad-hoc `invoke` scattered in views.

### IPC contract

- Commands live in `src-tauri/src/commands.rs` and are registered in `lib.rs` `generate_handler!`.
- Errors are returned as `Result<T, String>` (map with `.map_err(|e| e.to_string())`).
- UUIDs cross the boundary as **strings**; parse with `Uuid::parse_str` on the Rust side.
- **GameId wire format** (string): `acc` | `ac` | `lmu` | `f1_25`.  
  Rust enum uses `Acc`, `Ac`, `Lmu`, `F1_25`. Frontend normalizes via `gameIdFromRust` in `types.ts` / `api.ts`.

When adding a command: implement in `commands.rs`, register in `lib.rs`, add a typed wrapper in `api.ts`, and mirror types in `types.ts` if needed.

### Recording / storage invariants

- Distance-aligned samples use `DISTANCE_GRID_POINTS = 4000` (`sim-core`).
- Lap files are Parquet under `sessions/{session_id}/laps/{lap_id}.parquet`.
- Session lifecycle (`crates/daemon/src/recorder.rs`): a new session starts when `track_id` **or** car changes (`session_track_changed` / `session_car_changed`, both case-insensitive and requiring both sides non-empty); a live-physics freeze ≥ `LIVE_PHYSICS_TIMEOUT` (30 s) opens a **stint gap** (next lap → next stint, emits a "Stint N — break detected" notification when it actually splits) without ending the session; a freeze reaching `SESSION_ABANDON_TIMEOUT` (8 min) finalizes the session. A lap whose telemetry was truncated by a freeze is persisted **invalid** (`stint_gap_during_lap`).
- Adapters only announce a session / track / car change while **on track** (ACC `session_ready`; AC `graphics.status == LIVE`; LMU `active != 0`) — menu/replay/pause never spawns a session. ACC/AC/LMU re-emit `SessionInfo` on a `last_track_id` **or** `last_car` change. AC also treats a frozen `packet_id` as a heartbeat unless it stays frozen while the car is moving (`frozen_packet_still_live`).
- Lapless sessions are pruned: `finalize_session` drops a session with no laps, and `Database::open` sweeps orphaned lapless sessions on startup. `list_sessions` still returns an unfinalized lapless session (the one currently recording).
- `laps.stint_break_s` (nullable) holds the freeze seconds that opened a stint; only the first persisted lap of stints 2+ carries it. `.stb` bundles include it (still `bundle_version` 2 — additive optional). Stint separators (`src/lib/stints.ts`) render it as "… · N min break"; the Review separator also shows per-stint lap count / best / avg (`computeStintStatsMap`), and lap-number cells prefix `S{stint}·` when a session has >1 stint.
- Personal bests (top 3 valid laps per player/track) live in SQLite `leaderboard_laps` plus copies under `leaderboard/laps/`; deleting a session must not remove those rows or files.
- Portable export format is **`.stb`** (ZIP); see `docs/bundle-format.md` — bump/`bundle_version` carefully.
- ACC is the **reference adapter**; port parity (validity, sectors, track naming) from ACC when improving other games.

### UI structure

- Routes in `App.tsx`: `/`, `/leaderboard`, `/fuel`, `/compare`, `/compare/:sessionId`, `/settings`.
- Charts use **uPlot** / `uplot-react`; keep distance_pct as the shared X axis / cursor key.
- Global styles live mainly in `App.css` (no component CSS modules / Tailwind).

---

## Code-style guardrails

### TypeScript / React

- **Strict TypeScript** — do not weaken `tsconfig.json` (`strict`, `noUnusedLocals`, `noUnusedParameters`).
- Prefer functional components; colocate view-specific UI under `views/` or `components/`.
- Pure logic → `src/lib/`; keep React components thin.
- Match existing naming: `camelCase` functions, `PascalCase` components, `snake_case` fields that mirror Rust serde (`lap_time_ms`, `distance_pct`).
- Do not introduce a new chart library, CSS framework, or state library without an explicit request.
- Double quotes and trailing commas match current Vite/TS style in this repo.

### Rust

- Edition **2021**; use workspace deps from root `[workspace.dependencies]` when possible.
- Prefer `thiserror` for library errors; `anyhow` is fine at app/daemon boundaries.
- Capture adapters implement `GameAdapter` (`poll` → `AdapterEvent`); normalize controls with helpers in `sim-core` (`normalize_throttle`, `kmh_to_mps`, …).
- Shared memory is Windows-specific (`windows` crate in `capture-common`) — keep `unsafe` localized there.
- Unit tests: inline `#[cfg(test)]` modules next to the code (see `resample.rs`, `parquet_io.rs`, `schema.rs`).
- Avoid adding new top-level binaries unless packaging requires it; the app binary is `simtelemetry`.

### Docs / comments

- Prefer updating `docs/` (setup guides, bundle format, implementation plan) when changing user-facing capture or export behavior.
- Do not invent markdown docs the user did not ask for; `AGENTS.md` / requested docs are exceptions.

### Safety / scope

- Do not commit secrets, large `target/`/`node_modules` artifacts, or user session databases.
- Do not change installer targets (`msi`/`nsis`) or app identifier (`com.lowfive.simtelemetry`) casually.
- Keep capture logic read-only w.r.t. games (shared memory / UDP ingest only).

---

## Common change recipes

| Goal | Touch |
|------|--------|
| New UI page | `views/*`, route in `App.tsx`, Sidebar link |
| New IPC API | `commands.rs` + `lib.rs` + `api.ts` (+ `types.ts`) |
| New game field / sample channel | `sim-core` schema → storage Parquet schema → UI types/charts |
| Fix ACC/AC/LMU/F1 ingest | Matching `crates/capture-*` (+ tests) |
| Track map missing | `scripts/build-tracks.mjs` + `npm run build:tracks` |
| Bundle format change | `crates/storage` + `docs/bundle-format.md` + import validation |

---

## Key references

- `README.md` — quick start, data paths, troubleshooting
- `docs/implementation-plan.md` — maturity by game, ACC parity checklist
- `docs/game-setup/*.md` — per-game telemetry setup (also used by Setup Wizard)
- `docs/bundle-format.md` — `.stb` layout
- `installer/README.md` — MSI/NSIS output locations
)
