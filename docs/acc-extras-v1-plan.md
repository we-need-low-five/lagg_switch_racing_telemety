# ACC Extras v1 — Capture & UI Plan

_Status: agreed (grilling, Aug 2026). Do not expand scope without revisiting this doc._

## Goal

Capture a bounded set of ACC shared-memory fields and surface them on:

- **Review Session** (`/sessions/:sessionId`) — per-lap table columns
- **Analysis** (lap compare / `LapCompareView`) — distance-aligned charts

Other games (AC / LMU / F1) leave the new fields null/absent. UI still renders chrome; missing values use the empty-state rules below.

## Out of scope (v1)

- Brake temperatures
- Fuel Calculator page changes (stays fully manual)
- Backfilling already-recorded laps
- Promoting TC/ABS to Analysis series
- G-force, slip, weather, session type, and other ACC fields not listed below

---

## Field inventory

### Lap metadata (Review Session)

Snapshotted at **lap start** (first sample of the lap):

| Field | ACC source | Notes |
|-------|------------|--------|
| Tyre compound | `graphics.tyre_compound` | String |
| TC level | `graphics.tc_level` | Integer HUD step |
| ABS level | `graphics.abs_level` | Integer HUD step |

Derived at **lap end**:

| Field | Rule |
|-------|--------|
| Fuel used (L) | `fuel_remaining` at first sample − last sample; clamp ≥ 0 |

### Distance-aligned channels (Analysis / Parquet)

| Channel | ACC source | Store unit |
|---------|------------|------------|
| `fuel` | `physics.fuel` | Liters remaining |
| `tyre_temp_fl` … `tyre_temp_rr` | `physics.tyre_core_temp` | °C |
| `tyre_press_fl` … `tyre_press_rr` | `physics.wheel_pressure` | PSI |

Corner order (storage + UI left→right): **FL · FR · RL · RR**.

### Derived for Analysis display only

- **Fuel used (cumulative)** chart series: `fuel_start − fuel(t)` from the stored remaining series (not a separate Parquet column unless implementation prefers storing remaining only and deriving in UI — remaining must be persisted).

---

## Storage

### Approach

- **SQLite:** new nullable lap columns (or equivalent typed nullable fields) for compound, TC, ABS, fuel_used.
- **Parquet:** new optional float columns for `fuel` + 4 temps + 4 pressures.
- Old laps remain readable; missing columns → null / omit series.
- Schema is **first-class optional**, not an ACC-only opaque JSON blob.
- Channel list metadata in `lap_files` / schema JSON must be updated when writing new laps.

### `.stb` bundles

- Export **bundle version 2** including new lap meta and Parquet channels.
- **Import v1:** load what exists; **omit** ACC extras (no hard failure).
- Import v2: full extras when present.
- Update `docs/bundle-format.md` when implementing.

---

## Review Session UI

Always show columns (including non-ACC / old sessions):

| Column | Missing |
|--------|---------|
| Compound | `—` |
| TC | `—` |
| ABS | `—` |
| Fuel used | `—` |

Keep existing: Lap · Time · S1 · S2 · S3 · Δ Best.

---

## Analysis UI

### Vertical order (top → bottom)

1. Existing channels (speed, throttle, brake, steering, gear, RPM, time delta, segments/track map as today)
2. **Tyre core temps** — one row of **4** charts (FL FR RL RR)
3. **Tyre pressures** — one row of **4** charts (FL FR RL RR)
4. **Fuel used (cumulative)** — full-width, **collapsed by default**

### Tyre chart layout

- Two rows × four tiles.
- Each tile ~¼ the width of a current full chart; **compressed X-axis** so four fit in roughly one chart’s horizontal space.
- Still **eight separate charts** (not a single multi-series panel).
- Join the **same global `distance_pct` cursor** as the rest of Analysis.

### Fuel chart

- Full-width like Speed/Throttle.
- Default: **collapsed**.
- Y values: cumulative fuel used over the lap (`fuel_start − fuel(t)`).

### Missing-data behavior

| Situation | Behavior |
|-----------|----------|
| **Every** selected lap lacks that channel | Chart chrome stays; muted **“No data”** overlay |
| **Some** laps have data | Draw only laps that have data; **no** overlay |
| Review cells | Always `—` when null |

Do **not** plot literal `0.0` for missing series.

---

## Units & Settings

### Storage (always native ACC)

- Fuel: liters  
- Tyre temp: °C  
- Tyre pressure: PSI  

### Display toggles (add in Settings in this v1)

| Preference | Options | Default |
|------------|---------|---------|
| Fuel | L ↔ US gal | L |
| Temperature | °C ↔ °F | °C |
| Pressure | PSI ↔ bar | PSI |

Apply toggles in Review formatting and Analysis axis/tooltips (same pattern as existing `speedUnit`).

---

## Capture notes (ACC adapter)

- Extend `TelemetrySample` (and resample → `DistanceSample`) with the new numeric channels; stop relying on ephemeral `raw` JSON for anything we care about persisting.
- Lap-start snapshot: compound / TC / ABS when the lap begins (first telemetry after lap boundary).
- Fuel used computed on flush from first/last remaining samples of that lap’s buffer.
- Non-ACC adapters: leave new fields unset (`None` / NaN policy — pick one consistent approach at implement time; UI treats as missing).

---

## Implementation checklist (suggested order)

1. **Schema** — `sim-core` sample/lap types; SQLite migration; Parquet read/write + channel metadata.
2. **ACC adapter** — read fields; fill samples + lap-start meta; compute fuel used on lap complete.
3. **IPC / `types.ts` / `api.ts`** — expose new lap fields and sample channels.
4. **Review Session** — columns + formatting (respect unit prefs).
5. **Analysis** — 4+4 tyre grid, fuel chart (collapsed), cursor linking, No data overlay, unit prefs on axes.
6. **Settings** — three unit toggles + persistence in preferences.
7. **Bundles** — v2 export; v1 import compatibility; docs update.
8. **Tests** — resample/parquet roundtrip with new columns; fuel used clamp; sector/lap meta migration smoke.

---

## Success criteria

- New ACC sessions show Compound, TC, ABS, Fuel used on Review.
- Analysis shows tyre temp/pressure grids and a usable cumulative fuel-used chart for those laps.
- Old sessions and non-ACC games remain usable (dashes / No data / partial series).
- Unit toggles change display only; stored files stay L / °C / PSI.
- `.stb` v2 exports extras; v1 imports still load core lap data.
)
