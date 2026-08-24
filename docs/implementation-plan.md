# SimTelemetry — Implementation Plan

_Last updated: July 2026_

This document summarizes work completed so far, documents the ACC-specific fixes that made recording and analysis reliable, and lays out what still needs to be done—especially porting the same quality bar to Assetto Corsa, Le Mans Ultimate, and F1 25.

---

## 1. Project overview

**SimTelemetry** is a Windows desktop app (Tauri 2 + Rust workspace + React/TypeScript UI) that:

- Auto-detects running sim games and records lap telemetry in the background
- Stores sessions in SQLite + Parquet under `%LOCALAPPDATA%\SimTelemetry\`
- Provides a session browser and multi-lap analysis UI (charts, sector deltas, track map)
- Exports/imports portable `.stb` session bundles

**Supported games (adapters exist):**

| Game | Crate | Telemetry source | Maturity |
|------|-------|------------------|----------|
| Assetto Corsa Competizione | `capture-acc` | Shared memory (`acpmf_*`) | **Reference implementation** |
| Assetto Corsa | `capture-ac` | Shared memory | Basic — needs ACC parity |
| Le Mans Ultimate | `capture-lmu` | `LMU_Data` shared memory | Basic — needs ACC parity |
| F1 25 | `capture-f1` | UDP port 20777 | Basic — needs ACC parity |

---

## 2. Completed foundation (all games benefit)

These phases are done and form the shared platform:

### 2.1 Core platform

- [x] Tauri 2 app with system tray, recording daemon thread, SQLite + Parquet storage
- [x] `GameAdapter` trait + game detector routing
- [x] Session/lap schema, distance-grid resampling (`DISTANCE_GRID_POINTS = 4000`)
- [x] Recording service: lap start/complete, flush to Parquet, best-lap refresh, pin lap
- [x] IPC commands: `list_sessions`, `list_laps`, `load_lap_samples`, `get_session`, recording controls, bundle export/import

### 2.2 Analysis UI

- [x] Session browser
- [x] Lap comparison view: multi-lap channel overlays (speed, throttle, brake, steering, gear, RPM)
- [x] Time-delta chart vs reference lap
- [x] Sector delta bars
- [x] Linked cursor across charts (`distance_pct`)
- [x] Lap pinning and reference-lap selection (up to 4 laps)

### 2.3 Packaging & onboarding

- [x] Windows installer build (`npm run tauri:build`)
- [x] Game setup wizard with per-game telemetry checks
- [x] Setup docs under `docs/game-setup/`
- [x] `.stb` bundle export/import format

### 2.4 Track map infrastructure (game-agnostic)

- [x] Predetermined track layouts in `public/tracks/{track_id}.json` (normalized centerline polylines)
- [x] `scripts/build-tracks.mjs` + `npm run build:tracks` (TUM CSV, F1 GeoJSON, RaceTracksAPI SVG sources)
- [x] `src/lib/trackLayout.ts` — layout loading, path interpolation, `distance_pct` → map position
- [x] `TrackMap.tsx` — fixed outline + telemetry plotted by `distance_pct` (not world `pos_x`/`pos_y`)
- [x] `track_id` on `SessionInfo` / `SessionRecord` + DB column + `get_session` command
- [x] **22 ACC circuits** bundled (`public/tracks/index.json`)

---

## 3. ACC reference implementation — fixes and improvements

ACC is the only game where telemetry capture, metadata, lap validity, resampling, and analysis have been debugged end-to-end. Treat this section as the **template** for other adapters.

### 3.1 Shared memory & parsing

| Area | Problem | Fix |
|------|---------|-----|
| Physics sampling | Stale/deduped reads missed samples | Poll `parse_physics_map` directly each tick; emit on new `packet_id` |
| Player position | Used physics velocity / wrong source | Read `graphics.car_coordinates[player_car_id]` for `pos_x/y/z` |
| Statics | Track name wrong or empty | Read ACC statics at verified UTF-16 offsets (`acc_statics.rs`); use `track` field at offset 134 |
| Track display name | Showed garbage like "ack config" | Stop preferring misaligned `trackConfiguration`; map raw id → display via `TRACK_DISPLAY_NAMES` |
| Session gating | Sessions created before game was ready | Only emit `SessionInfo` when graphics status is active **and** track + car names resolve |

**Files:** `crates/capture-acc/src/adapter.rs`, `crates/capture-acc/src/acc_statics.rs`

### 3.2 Session & lap metadata

| Area | Fix |
|------|-----|
| `track_id` | Store ACC internal id (e.g. `monza`) from statics `track` field — used for track map layout lookup |
| `track` | Human-readable name via `resolve_track_name()` |
| Car / player | From statics `car_model`, `player_nick` (fallback: name + surname) |
| Session creation | Recorder waits for non-empty `track` + `car` before `create_session` |

### 3.3 Lap validity

| Rule | Implementation |
|------|----------------|
| Invalid if game says so | `graphics.is_valid_lap == false` → mark lap invalid |
| Invalid if pit involved | `is_in_pit_lane` or `is_in_pit` during lap → invalid |
| Sector times | Track `current_sector_index` + `last_sector_time` on sector transitions |

### 3.4 Recording pipeline

| Area | Problem | Fix |
|------|---------|-----|
| Empty Analyze tab | Recording auto-paused on heartbeats; ~1 KB Parquet files | Removed auto-pause on heartbeat; unpause on session start |
| Sparse laps | Laps saved with too few samples | `flush_lap` logs warning when sample count too low |
| Distance alignment | Position-based distance degenerate (zero movement in x/y/z) | `resample.rs` falls back to `distance_m` delta when position path length is zero |

### 3.5 Track map

| Area | Problem | Fix |
|------|---------|-----|
| Wrong circuit shape | Map drew polyline from telemetry `pos_x`/`pos_y` | Load fixed layout JSON; place dots/cursor by `distance_pct` along centerline |
| Missing track id on old sessions | N/A | Frontend `resolveTrackId()` falls back from display name → id |

**Files:** `src/components/charts/TrackMap.tsx`, `src/lib/trackLayout.ts`, `src/views/LapCompare.tsx`, `public/tracks/*`

---

## 4. Per-game gap analysis (ACC parity work)

Each non-ACC adapter needs the same categories of work ACC received. Status below reflects current codebase.

### 4.1 Assetto Corsa (`capture-ac`)

| Category | Status | Required work |
|----------|--------|----------------|
| Shared memory maps | Partial | Verify physics/graphics/statics struct layouts and offsets against AC shared memory spec |
| Position source | Unknown | Confirm best world-position source (graphics vs physics); align with ACC approach |
| `track_id` | Slugified display name | Read raw AC track id from statics if available; maintain `TRACK_DISPLAY_NAMES` map |
| Track / car names | Basic UTF-16 read | Validate no garbage strings; defer session until names populated |
| Lap detection | `completed_laps` counter | Verify pit/invalid lap rules for AC graphics fields |
| Sector times | Not implemented | Wire AC sector fields if exposed in graphics |
| Sampling rate | Unknown | Ensure no dedup drops samples; log sample count per lap |
| Track layouts | None bundled | Add AC track id → layout mapping; extend `build-tracks.mjs` or separate `public/tracks-ac/` |
| Tests | None | Unit tests for statics parsing + lap validity rules |

### 4.2 Le Mans Ultimate (`capture-lmu`)

| Category | Status | Required work |
|----------|--------|----------------|
| Shared memory | `LMU_Data` struct | Validate struct layout against LMU 1.2+ docs; handle version changes |
| `track_id` | Slugified name | Use LMU internal track identifier if exposed in shared memory |
| Position | From telemetry struct | Verify `pos_x/y/z` are world coords suitable for resampling fallback |
| Lap / pit / valid | Basic `in_pits` check | Map LMU invalid-lap and pit-lane flags equivalent to ACC |
| Sector times | Partial (sector index) | Confirm sector time fields and indexing |
| Session gating | Announces immediately | Gate on active session + valid track/car strings |
| Track layouts | None bundled | LMU shares many ACC circuits — reuse ACC layouts where ids match; add LMU-only tracks |
| Tests | None | Adapter integration tests with mock shared memory |

### 4.3 F1 25 (`capture-f1`)

| Category | Status | Required work |
|----------|--------|----------------|
| Telemetry | UDP packets | Verify packet types, motion vs telemetry timing, lap/time fields |
| Session metadata | Placeholder | Parse real track name, car, player from UDP session/motion packets |
| `track_id` | Empty string | Map F1 track id → canonical layout id |
| Position | Motion packet | Confirm coordinates and distance for resampling |
| Lap validity | `current_lap_invalid` | Extend with pit/sector rules as available |
| Sector times | Not wired | Parse sector times from lap data packet |
| Track layouts | None bundled | F1 calendar circuits — new layout set or shared geo sources |
| Tests | None | UDP fixture replay tests |

---

## 5. Cross-cutting remaining work

### 5.1 Track layout system

- [x] ACC: 22 circuits in `public/tracks/`
- [ ] **Namespace layouts by game** if ids collide (e.g. `public/tracks/acc/monza.json` vs `ac/monza`)
- [ ] AC: build layout index for popular AC content (base + DLC tracks)
- [ ] LMU: reuse ACC layouts where track ids align; add LMU-exclusive circuits
- [ ] F1 25: layout set for current season calendar
- [ ] Bundle export: embed `track_id` + reference layout path in manifest (today `track.json` in bundle still uses sampled positions — should reference bundled layout)
- [ ] Document `npm run build:tracks` in README

### 5.2 Resampling & analysis

- [ ] Prefer game-native normalized lap position (ACC `normalized_car_position`) for `distance_pct` when available — more accurate than position integration
- [ ] Sector boundaries as % along lap for map markers
- [ ] Handle out-lap / in-lap filtering in UI
- [ ] Interpolation quality audit per game (compare distance_pct monotonicity)

### 5.3 Recording robustness

- [ ] Per-adapter sample-rate metrics in session metadata
- [ ] Reconnect handling: game restart mid-session vs new session
- [ ] Optional: split invalid / out-lap recording policy (record but tag vs skip)

### 5.4 Data model & migration

- [x] `track_id` column with migration for existing DBs
- [ ] Backfill `track_id` for old sessions from `track` display name where possible
- [ ] Bundle format version bump if session schema changes again

### 5.5 UI / UX

- [ ] Show track layout source / missing-layout hint on Analyze tab
- [ ] Session list: show `track_id` or layout availability indicator
- [ ] Live recording indicator: current track, lap, sample count
- [ ] Compare view: handle AC/LMU/F1 sessions with no bundled layout gracefully (already shows message)

### 5.6 Quality & CI

- [ ] Rust unit tests per adapter (statics parsing, lap events)
- [ ] Golden-file tests for resampling
- [ ] CI: `cargo test`, `npm run build`, `npm run build:tracks`
- [ ] Manual test matrix per game (see §7)

---

## 6. Recommended implementation phases

### Phase A — ACC hardening (short)

_Goal: lock ACC as the golden path._

1. Use ACC `normalized_car_position` for distance grid when present
2. Add sector % markers on track map
3. README + troubleshooting updates (re-record sessions after fixes)
4. CI smoke tests for `capture-acc` and `resample`

**Exit criteria:** New ACC session → correct track name, non-empty Parquet, valid/invalid laps correct, track map matches circuit.

### Phase B — AC parity

_Goal: AC sessions are as trustworthy as ACC._

1. Audit AC shared memory structs (physics, graphics, statics)
2. Fix position, lap validity, sector times, session gating (mirror ACC §3)
3. Resolve `track_id` from AC statics
4. Bundle AC track layouts (start with top 20 mod/base tracks)
5. Manual test: 3 tracks × 5 laps, verify Analyze tab

**Exit criteria:** AC session analysis matches ACC quality bar.

### Phase C — LMU parity

_Goal: same as Phase B for Le Mans Ultimate._

1. Validate `LmuTelemetry` layout against current LMU build
2. Apply ACC lap validity + session gating patterns
3. Map LMU track ids → layouts (reuse ACC JSON where ids match)
4. Setup wizard validation against real LMU process

**Exit criteria:** LMU recording and analysis work on shared ACC circuits.

### Phase D — F1 25 parity

_Goal: real metadata and layouts for F1 25._

1. Parse session/track/car from UDP (not placeholders)
2. Lap + sector from F1 lap data packets
3. F1 track layout set
4. Document UDP settings in setup wizard (already partially done)

**Exit criteria:** F1 25 session shows correct circuit name and track map.

### Phase E — Polish & release

1. Layout namespacing by game if needed
2. Bundle format v2 (embedded layout reference)
3. Installer + friend onboarding pass
4. Full manual test matrix (§7)

---

## 7. Verification checklist (per game)

Run after each adapter parity pass:

| Check | How to verify |
|-------|----------------|
| Game detection | Launch game → tray shows recording notification |
| Session metadata | DB row has correct `track_id`, `track`, `car`, `player_name` |
| Sample rate | Parquet file >> 1 KB; ~4000 rows per lap |
| Lap validity | Pit lap marked invalid; clean lap marked valid |
| Best lap | Fastest valid lap gets `is_best` |
| Charts | All channels show data on Analyze tab |
| Track map | Correct circuit outline; speed dots follow track |
| Cursor sync | Moving cursor on chart updates map position |
| Bundle | Export `.stb` → import on second machine → analysis intact |

---

## 8. Key files reference

| Area | Path |
|------|------|
| ACC adapter | `crates/capture-acc/src/adapter.rs` |
| ACC statics / track names | `crates/capture-acc/src/acc_statics.rs` |
| AC adapter | `crates/capture-ac/src/adapter.rs` |
| LMU adapter | `crates/capture-lmu/src/adapter.rs` |
| F1 adapter | `crates/capture-f1/src/adapter.rs` |
| Recording | `crates/daemon/src/recorder.rs` |
| Resampling | `crates/core/src/resample.rs` |
| Schema / DB | `crates/core/src/schema.rs`, `crates/storage/src/database.rs` |
| Track layouts | `public/tracks/`, `scripts/build-tracks.mjs` |
| Track map UI | `src/components/charts/TrackMap.tsx`, `src/lib/trackLayout.ts` |
| Analyze view | `src/views/LapCompare.tsx` |
| IPC | `src-tauri/src/commands.rs` |

---

## 9. Notes for developers

- **Re-record after fixes:** Sessions created before telemetry/recording fixes may have empty Parquet or wrong metadata. Do not debug analysis with old sessions.
- **ACC is the template:** When in doubt, compare behavior to `capture-acc` and port the same pattern.
- **Track map depends on `track_id`:** Layout lookup uses `public/tracks/{track_id}.json`. Display-name fallback exists but storing the correct id at record time is preferred.
- **Distance alignment is layout-independent:** Charts use `distance_pct`; track map now uses the same percentage along a fixed centerline. World coordinates are still stored but are no longer used for map geometry.
