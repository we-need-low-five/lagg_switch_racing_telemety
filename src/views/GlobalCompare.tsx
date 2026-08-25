import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation } from "react-router-dom";
import {
  listLeaderboardGames,
  listLeaderboardTracks,
  listTrackLaps,
  loadLapSamples,
} from "../api";
import { LapCompareView } from "../components/compare/LapCompareView";
import { TrackLapPicker } from "../components/compare/TrackLapPicker";
import {
  mergeCatalog,
  trackLapOptionToMeta,
  type CompareLapMeta,
} from "../lib/compareLaps";
import {
  loadTrackLayout,
  resolveTrackId,
  type TrackLayout,
} from "../lib/trackLayout";
import type { DistanceSample, GameId, LeaderboardTrackOption } from "../types";

function trackOptionKey(track: LeaderboardTrackOption): string {
  return `${track.track_id}|${track.track}`;
}

function parseTrackKey(key: string): { trackId: string; trackName: string } {
  const idx = key.indexOf("|");
  if (idx === -1) return { trackId: key, trackName: key };
  return {
    trackId: key.slice(0, idx),
    trackName: key.slice(idx + 1),
  };
}

interface GlobalCompareNavState {
  game?: GameId;
  trackKey?: string;
  lapIds?: string[];
}

export function GlobalCompare() {
  const location = useLocation();
  const navState = (location.state as GlobalCompareNavState | null) ?? null;
  const [games, setGames] = useState<GameId[]>([]);
  const [tracks, setTracks] = useState<LeaderboardTrackOption[]>([]);
  const [trackLaps, setTrackLaps] = useState<Awaited<ReturnType<typeof listTrackLaps>>>([]);
  const [selectedGame, setSelectedGame] = useState<GameId | "">("");
  const [selectedTrackKey, setSelectedTrackKey] = useState("");
  const [pickerOpen, setPickerOpen] = useState(true);
  const [draftIds, setDraftIds] = useState<string[]>([]);
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [catalogMetas, setCatalogMetas] = useState<CompareLapMeta[]>([]);
  const [samples, setSamples] = useState<Record<string, DistanceSample[]>>({});
  const [referenceId, setReferenceId] = useState<string | null>(null);
  const [trackLayout, setTrackLayout] = useState<TrackLayout | null>(null);
  const [layoutLoading, setLayoutLoading] = useState(false);
  const [loadingGames, setLoadingGames] = useState(true);
  const [loadingLaps, setLoadingLaps] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadGames = useCallback(async () => {
    try {
      setError(null);
      const rows = await listLeaderboardGames();
      setGames(rows);
      setSelectedGame((prev) => {
        if (navState?.game && rows.includes(navState.game)) return navState.game;
        if (prev && rows.includes(prev)) return prev;
        return rows[0] ?? "";
      });
    } catch (e) {
      setError(String(e));
      setGames([]);
      setSelectedGame("");
    } finally {
      setLoadingGames(false);
    }
  }, [navState?.game]);

  useEffect(() => {
    loadGames();
  }, [loadGames]);

  useEffect(() => {
    if (!selectedGame) {
      setTracks([]);
      setSelectedTrackKey("");
      return;
    }
    let cancelled = false;
    listLeaderboardTracks(selectedGame)
      .then((rows) => {
        if (cancelled) return;
        setTracks(rows);
        const keys = rows.map(trackOptionKey);
        setSelectedTrackKey((prev) => {
          if (navState?.trackKey && keys.includes(navState.trackKey)) {
            return navState.trackKey;
          }
          if (prev && keys.includes(prev)) return prev;
          return keys[0] ?? "";
        });
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedGame, navState?.trackKey]);

  useEffect(() => {
    if (!selectedGame || !selectedTrackKey) {
      setTrackLaps([]);
      setCatalogMetas([]);
      setDraftIds([]);
      setCompareIds([]);
      setReferenceId(null);
      setTrackLayout(null);
      setPickerOpen(true);
      return;
    }
    const { trackId, trackName } = parseTrackKey(selectedTrackKey);
    let cancelled = false;
    setLoadingLaps(true);
    setDraftIds([]);
    setCompareIds([]);
    setReferenceId(null);
    setSamples({});
    setPickerOpen(true);

    const trackIdResolved = resolveTrackId(trackId, trackName);
    if (trackIdResolved) {
      setLayoutLoading(true);
      loadTrackLayout(trackIdResolved)
        .then((layout) => {
          if (!cancelled) setTrackLayout(layout);
        })
        .catch(() => {
          if (!cancelled) setTrackLayout(null);
        })
        .finally(() => {
          if (!cancelled) setLayoutLoading(false);
        });
    } else {
      setTrackLayout(null);
      setLayoutLoading(false);
    }

    listTrackLaps(selectedGame, trackId, trackName)
      .then((rows) => {
        if (cancelled) return;
        setTrackLaps(rows);
        const metas = rows.map((row) => trackLapOptionToMeta(row));
        setCatalogMetas(metas);
        const ids = (navState?.lapIds ?? []).filter((id) =>
          rows.some((row) => row.lap_id === id),
        );
        if (ids.length > 0) {
          setDraftIds(ids);
          setCompareIds(ids);
          setReferenceId(ids[0]);
          setPickerOpen(false);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingLaps(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedGame, selectedTrackKey, navState?.lapIds]);

  useEffect(() => {
    if (compareIds.length === 0) {
      setSamples({});
      return;
    }
    let cancelled = false;
    Promise.all(
      compareIds.map(async (id) => [id, await loadLapSamples(id)] as const),
    )
      .then((entries) => {
        if (cancelled) return;
        setSamples((prev) => {
          const map = { ...prev };
          for (const [id, data] of entries) map[id] = data;
          return map;
        });
      })
      .catch((e) => setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [compareIds]);

  useEffect(() => {
    if (referenceId && !compareIds.includes(referenceId)) {
      setReferenceId(compareIds[0] ?? null);
    }
    if (!referenceId && compareIds.length > 0) {
      setReferenceId(compareIds[0]);
    }
  }, [compareIds, referenceId]);

  const catalog = useMemo(
    () => mergeCatalog(catalogMetas, []),
    [catalogMetas],
  );

  function handleCompare() {
    if (draftIds.length === 0) return;
    setCompareIds(draftIds);
    setReferenceId(draftIds[0]);
    setPickerOpen(false);
  }

  function handleReopenPicker() {
    setDraftIds(compareIds);
    setPickerOpen(true);
  }

  return (
    <div className="page compare-page global-compare-page">
      <div className="page-inner">
        <header className="page-header compare-page-header">
          <div>
            <h1>Compare</h1>
            <p className="subtitle">
              Pick laps from any session or kept leaderboard telemetry on the
              same track.
            </p>
          </div>
        </header>

        {pickerOpen ? (
          <TrackLapPicker
            games={games}
            tracks={tracks}
            laps={trackLaps}
            selectedGame={selectedGame}
            selectedTrackKey={selectedTrackKey}
            selectedIds={draftIds}
            loadingGames={loadingGames}
            loadingLaps={loadingLaps}
            onGameChange={(game) => {
              setSelectedGame(game);
              setDraftIds([]);
              setCompareIds([]);
              setReferenceId(null);
            }}
            onTrackChange={(key) => {
              setSelectedTrackKey(key);
              setDraftIds([]);
              setCompareIds([]);
              setReferenceId(null);
            }}
            onSelectedIdsChange={setDraftIds}
            onCompare={handleCompare}
          />
        ) : (
          <div className="global-compare-picker-summary">
            <span>
              {compareIds.length} lap{compareIds.length !== 1 ? "s" : ""} compared
            </span>
            <button
              type="button"
              className="secondary lap-panel-reopen"
              onClick={handleReopenPicker}
            >
              Change laps
            </button>
          </div>
        )}
      </div>

      <div className="compare-view-root">
        <LapCompareView
          mode="global"
        catalog={catalog}
        selectedIds={compareIds}
        referenceId={referenceId}
        samples={samples}
        game={selectedGame || null}
        trackLayout={trackLayout}
        layoutLoading={layoutLoading}
        error={error}
        onOpenLapPicker={handleReopenPicker}
        lapPanel={
          compareIds.length > 0 ? (
            <div className="lap-panel global-ref-panel">
              <p className="muted small">Reference lap</p>
              <div className="lap-list-scroll">
                {compareIds
                  .map((id) => catalog.find((m) => m.lapId === id))
                  .filter((m): m is CompareLapMeta => m != null)
                  .map((meta) => (
                    <label key={meta.lapId} className="lap-option">
                      <input
                        type="radio"
                        name="global-ref"
                        checked={referenceId === meta.lapId}
                        onChange={() => setReferenceId(meta.lapId)}
                      />
                      <div>
                        <strong>{meta.playerName}</strong>
                        <span>{meta.car}</span>
                      </div>
                    </label>
                  ))}
              </div>
            </div>
          ) : null
        }
        />
      </div>
    </div>
  );
}
