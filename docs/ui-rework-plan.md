# SimTelemetry — UI Rework Plan

_Decisions locked via product grilling · August 2026_

This plan covers four related UI changes: session entry/review flow, distance-segment comparison, resizable analysis layout, and appearance settings. Do not implement until this document is treated as the agreed scope.

---

## Goals

1. **Sessions** — Cleaner cards; Review session as the default entry into a session.
2. **Splits** — Replace track-sector (S1/S2/S3) *comparison* with 10 equal-distance segments.
3. **Analysis layout** — Collapse and resize charts, track map, and laps panel.
4. **Appearance** — Theme background (presets / custom / gradient) driving chrome contrast, plus editable graph colors.

---

## 1. Sessions tab & Review session

### Product decisions

| Decision | Choice |
|----------|--------|
| Card primary CTA | **Review session** (replaces Analyze) |
| Export / Delete | Hidden behind **⋮ overflow menu** (not right-click-only) |
| Delete safety | Confirm dialog before delete |
| Review route | `/sessions/:sessionId` |
| Analysis route | Unchanged: `/compare/:sessionId` |
| Global compare | Unchanged: `/compare` |
| Direct card → charts | **No** — always via review (or explicit Compare from table) |

### Review session page (`/sessions/:sessionId`)

Laps table columns:

- Lap number
- Full lap time
- Actual game **S1 / S2 / S3** times (metadata from capture; not the new segment model)
- Delta vs **session best** lap
- Invalid laps: **red text**

Interaction:

- **Row click** → open analysis with that lap selected
- **Checkboxes** + **Compare selected** → multi-lap analysis (≥2)
- Pass selection into `/compare/:sessionId` via navigation state or query params (implementation detail; prefer typed state)

### Implementation touchpoints

| Area | Likely files |
|------|----------------|
| Session cards | `src/views/Sessions.tsx`, `src/App.css` |
| Overflow menu | New small UI helper or inline menu in `Sessions.tsx` |
| Route | `src/App.tsx` |
| Review view | New `src/views/SessionReview.tsx` (or similar) |
| IPC | Existing `list_laps` / `get_session` via `src/api.ts` |

### Acceptance

- [ ] Session card shows Review session + ⋮ (Export, Delete)
- [ ] Delete asks for confirmation
- [ ] Review page lists laps with columns above; invalid = red
- [ ] Row click and Compare selected both land in analysis with correct selection
- [ ] No Analyze button on cards

---

## 2. Ten equal-distance segments (replace sector compare)

### Product decisions

| Decision | Choice |
|----------|--------|
| Segment definition | **Equal distance** — 10 windows of ~10% `distance_pct` each `[0–10), …, [90–100]` |
| Real S1/S2/S3 in analysis UI | **Removed** (delta bars, zoom tabs, chart sector filters) |
| Real S1/S2/S3 elsewhere | **Kept** on Review session table only (and stored lap metadata) |
| Dual mode (sectors vs segments) | **No** |
| Analysis UI | Compact **10-cell delta strip**; **click segment** zooms charts to that window; clear selection → Full lap |

### Computation (guidance)

- Derive segment elapsed times from distance-aligned samples (interpolate `time` / lap elapsed at 0%, 10%, …, 100%).
- Delta strip: `compare − reference` per segment (same sign convention as today’s sector bars: negative = faster).
- Prefer pure TS in `src/lib/` (e.g. extend/replace `sectors.ts`); reuse 1000-point grid already in samples where possible.
- Chart zoom: filter/remap by `distance_pct` range for the selected segment (same mechanism as current sector zoom, different boundaries).

### Implementation touchpoints

| Area | Likely files |
|------|----------------|
| Segment math | `src/lib/sectors.ts` → rename/split to segments helper |
| Delta UI | `src/components/charts/SectorDeltaBar.tsx` → segment strip |
| Compare shell | `src/components/compare/LapCompareView.tsx` |
| Chart filters | `DistanceChart.tsx` / compare view sector tab logic |
| Normalize helpers | `src/lib/compareLaps.ts` (drop analysis dependence on sector tabs) |

Game adapters and stored `sector_*` fields stay as-is for Review table display; no capture-format change required for this plan.

### Acceptance

- [ ] Analysis shows 10-segment deltas vs reference; no S1/S2/S3 compare chrome
- [ ] Clicking a segment zooms linked charts to that 10% distance window
- [ ] Clearing selection restores full-lap charts
- [ ] Review table still shows game S1/S2/S3
- [ ] Leaderboard / other views do not reintroduce sector compare UI

---

## 3. Analysis layout: collapse & resize

### Product decisions

| Decision | Choice |
|----------|--------|
| Charts | Each chart **collapsible** + **free drag** height |
| Main columns | **Vertical splitter**: charts \| map+laps |
| Right column | **Horizontal splitter** between track map and laps; each **collapsible** |
| Persistence | Sizes + collapsed flags in `localStorage` (with preferences) |

### Implementation touchpoints

| Area | Likely files |
|------|----------------|
| Compare layout | `src/components/compare/LapCompareView.tsx`, `src/App.css` |
| Charts | `src/components/charts/DistanceChart.tsx` (height prop / resize) |
| Map / laps | `TrackMap.tsx`, `LapPanel.tsx` |
| Prefs | `src/lib/preferences.ts` |

Default chart height today is ~280px; use that as the initial default before user overrides.

### Acceptance

- [ ] User can collapse/expand each graph independently
- [ ] User can drag to change each graph’s height
- [ ] Column width adjustable via splitter
- [ ] Map vs laps height adjustable via splitter; both collapsible
- [ ] Layout survives app reload

---

## 4. Appearance settings

### Product decisions

| Decision | Choice |
|----------|--------|
| Background | **Presets** + **custom color** + **gradient on/off** |
| Chrome | Background **drives** panels, borders, text (not fill-only) |
| Light / bright bases | **Auto-derive contrast** (dark text on light, etc.) |
| Presets | Mostly dark; default remains current slate-like look |
| Graph colors | Editable **lap series** colors + **channel** colors + **Reset** |
| Scope | No separate named Light/Dark product modes beyond derivation |

### Settings UI

Add an Appearance (or Charts & appearance) section under Settings:

- Background preset swatches
- Custom color control
- Gradient toggle
- Lap color editors
- Channel color editors (Speed, Throttle, Brake, Steering, Gear, RPM, Time Delta)
- Reset to defaults

### Implementation touchpoints

| Area | Likely files |
|------|----------------|
| Settings | `src/views/Settings.tsx` |
| Prefs | `src/lib/preferences.ts` |
| Tokens / apply theme | `src/App.css`, app root (`App.tsx` or theme helper) |
| Chart colors | `DistanceChart.tsx` (`CHANNELS`, `LAP_COLORS`) — read from prefs |
| Segment delta colors | Keep functional green/red unless later extended; not required in v1 of this plan |

Introduce CSS variables for surfaces/text/borders derived from the chosen base; apply graph colors via prefs consumed by chart components.

### Acceptance

- [ ] User can pick preset or custom background and toggle gradient
- [ ] Panels/text remain readable on light and dark bases
- [ ] Lap and channel colors editable and applied in analysis
- [ ] Reset restores defaults
- [ ] Preferences persist across restarts

---

## Suggested implementation order

1. **Review session route + sessions card ⋮** — unblocks new entry flow without touching compare math.
2. **10-segment compare + zoom** — replace sector analysis chrome.
3. **Layout splitters / collapse** — analysis ergonomics.
4. **Appearance settings** — theme tokens + graph color prefs.

Ship each step in a usable state before starting the next when possible.

---

## Out of scope (this plan)

- Changing capture adapters or Parquet schema for segments
- Dual sector/segment compare mode
- Soft-delete / undo for sessions
- Full design-system rewrite beyond derived chrome tokens
- Customizing segment delta green/red (unless pulled into graph-color work later)
- Right-click context menus on session cards

---

## Key references (current code)

- Sessions: `src/views/Sessions.tsx`
- Compare: `src/views/LapCompare.tsx`, `src/components/compare/LapCompareView.tsx`
- Sectors: `src/lib/sectors.ts`, `src/components/charts/SectorDeltaBar.tsx`
- Prefs: `src/lib/preferences.ts`
- Settings: `src/views/Settings.tsx`
- Chart colors: `src/components/charts/DistanceChart.tsx`
- Architecture: `AGENTS.md`
