# Session Bundle Format (.stb)

Current version: **2** (v1 bundles still import)

`.stb` files are ZIP archives containing:

| Entry | Description |
|-------|-------------|
| `manifest.json` | Session metadata, lap list, flags |
| `laps/{lap_id}.parquet` | Distance-aligned lap samples |
| `track.json` | Optional track outline from best lap |

## manifest.json

### Version 1 (legacy)

Core lap metadata and 11-channel Parquet files only.

```json
{
  "bundle_version": 1,
  "session": { "id": "...", "game": "Acc", "track": "...", "car": "..." },
  "laps": [
    {
      "id": "...",
      "lap_number": 3,
      "lap_time_ms": 104321,
      "valid": true,
      "is_best": true,
      "is_pinned": false,
      "sectors": { "s1_ms": 32100, "s2_ms": 40200, "s3_ms": 32021 },
      "sample_rate_hz": 60.0
    }
  ]
}
```

Import loads core data; ACC extras (compound, TC/ABS, fuel used, tyre/fuel channels) are omitted without error.

### Version 2

Adds optional lap metadata and extended Parquet channels (when recorded from ACC):

| Lap field | Type | Notes |
|-----------|------|-------|
| `tyre_compound` | string \| null | Snapshotted at lap start |
| `tc_level` | integer \| null | HUD TC step at lap start |
| `abs_level` | integer \| null | HUD ABS step at lap start |
| `fuel_used_l` | number \| null | Liters consumed (lap end) |

Optional Parquet columns (native units: L, °C, PSI, G, degrees):

- `fuel` — liters remaining
- `tyre_temp_fl`, `tyre_temp_fr`, `tyre_temp_rl`, `tyre_temp_rr`
- `tyre_press_fl`, `tyre_press_fr`, `tyre_press_rl`, `tyre_press_rr`
- `g_force_x`, `g_force_y`, `g_force_z` — ACC/AC g-force (x=lat, y=vert, z=long)
- `slip_angle_fl`, `slip_angle_fr`, `slip_angle_rl`, `slip_angle_rr` — ACC slip angle (°)

Corner order: **FL · FR · RL · RR**.

```json
{
  "bundle_version": 2,
  "session": { "id": "...", "game": "Acc", "track": "...", "car": "..." },
  "laps": [
    {
      "id": "...",
      "lap_number": 3,
      "lap_time_ms": 104321,
      "valid": true,
      "is_best": true,
      "is_pinned": false,
      "sectors": { "s1_ms": 32100, "s2_ms": 40200, "s3_ms": 32021 },
      "sample_rate_hz": 60.0,
      "tyre_compound": "Dry",
      "tc_level": 5,
      "abs_level": 3,
      "fuel_used_l": 2.4
    }
  ]
}
```

Import accepts `bundle_version` 1 or 2. Versions above the app’s supported version are rejected.

Export always writes version **2** with whatever fields and Parquet columns exist for each lap.
