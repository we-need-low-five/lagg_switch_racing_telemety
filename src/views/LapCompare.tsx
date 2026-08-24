import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useLocation, useParams } from "react-router-dom";
import {
  getSession,
  listLaps,
  listTrackLaps,
  loadLapSamples,
  pinLap,
} from "../api";
import { LapPanel } from "../components/LapPanel";
import { AddExternalLapModal } from "../components/compare/AddExternalLapModal";
import { LapCompareView } from "../components/compare/LapCompareView";
import {
  canAddLap,
  lapRecordToMeta,
  mergeCatalog,
  toggleSelectedId,
  trackLapOptionToMeta,
  type CompareLapMeta,
} from "../lib/compareLaps";
import {
  loadTrackLayout,
  resolveTrackId,
  type TrackLayout,
} from "../lib/trackLayout";
import type { DistanceSample, TrackLapOption } from "../types";

interface CompareNavState {
  lapIds?: string[];
  referenceId?: string;
}

export function LapCompare() {
  const { sessionId } = useParams();
  const location = useLocation();
  const navState = (location.state as CompareNavState | null) ?? null;
  const [laps, setLaps] = useState<Awaited<ReturnType<typeof listLaps>>>([]);
  const [sessionMeta, setSessionMeta] = useState<
    Awaited<ReturnType<typeof getSession>>
  >(null);
  const [pickerOpen, setPickerOpen] = useState(true);
  const [draftIds, setDraftIds] = useState<string[]>([]);
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [externalMetas, setExternalMetas] = useState<CompareLapMeta[]>([]);
  const [samples, setSamples] = useState<Record<string, DistanceSample[]>>({});
  const [referenceId, setReferenceId] = useState<string | null>(null);
  const [trackLayout, setTrackLayout] = useState<TrackLayout | null>(null);
  const [layoutLoading, setLayoutLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [trackLaps, setTrackLaps] = useState<TrackLapOption[]>([]);
  const [trackLapsLoading, setTrackLapsLoading] = useState(false);
  const [navApplied, setNavApplied] = useState(false);

  useEffect(() => {
    if (!sessionId) return;
    getSession(sessionId)
      .then((session) => {
        setSessionMeta(session);
        const trackId = resolveTrackId(session?.track_id, session?.track);
        if (!trackId) {
          setTrackLayout(null);
          return;
        }
        setLayoutLoading(true);
        return loadTrackLayout(trackId).then((layout) => {
          setTrackLayout(layout);
        });
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLayoutLoading(false));
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;
    listLaps(sessionId)
      .then((rows) => {
        setLaps(rows);
        if (navState?.lapIds?.length && !navApplied) {
          const ids = navState.lapIds.filter((id) =>
            rows.some((l) => l.id === id),
          );
          if (ids.length > 0) {
            const best = rows.find((l) => l.is_best);
            const ref =
              navState.referenceId && ids.includes(navState.referenceId)
                ? navState.referenceId
                : best?.id ?? ids[0];
            setDraftIds(ids);
            setCompareIds(ids);
            setReferenceId(ref);
            setPickerOpen(false);
            setNavApplied(true);
            return;
          }
        }
        const best = rows.find((l) => l.is_best) ?? rows[rows.length - 1];
        const latest = rows[rows.length - 1];
        const defaults = [best?.id, latest?.id].filter(Boolean) as string[];
        const draft = Array.from(new Set(defaults)).slice(0, 4);
        setDraftIds(draft);
        setReferenceId(best?.id ?? null);
      })
      .catch((e) => setError(String(e)));
  }, [sessionId, navState, navApplied]);

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

  const sessionLapMetas = useMemo(() => {
    if (!sessionMeta) return [];
    return laps.map((lap) =>
      lapRecordToMeta(
        lap,
        sessionMeta.player_name,
        sessionMeta.car,
        sessionMeta.started_at,
        false,
      ),
    );
  }, [laps, sessionMeta]);

  const catalog = useMemo(
    () => mergeCatalog(sessionLapMetas, externalMetas),
    [sessionLapMetas, externalMetas],
  );

  const loadTrackLapsForModal = useCallback(async () => {
    if (!sessionMeta) return;
    setTrackLapsLoading(true);
    try {
      const rows = await listTrackLaps(
        sessionMeta.game,
        sessionMeta.track_id ?? "",
        sessionMeta.track,
      );
      setTrackLaps(rows);
    } catch (e) {
      setError(String(e));
    } finally {
      setTrackLapsLoading(false);
    }
  }, [sessionMeta]);

  function openModal() {
    setModalOpen(true);
    loadTrackLapsForModal();
  }

  function toggleLap(id: string) {
    setDraftIds((current) => {
      const next = toggleSelectedId(current, id);
      if (!next.includes(id) && referenceId === id) {
        setReferenceId(next[0] ?? null);
      }
      return next;
    });
  }

  function addExternalLap(lapId: string) {
    const sessionLap = laps.find((l) => l.id === lapId);
    if (sessionLap) {
      setDraftIds((prev) => toggleSelectedId(prev, lapId));
      return;
    }
    const option = trackLaps.find((l) => l.lap_id === lapId);
    if (!option) return;
    if (!canAddLap(draftIds.length)) return;

    const meta = trackLapOptionToMeta(option);
    setExternalMetas((prev) => {
      if (prev.some((m) => m.lapId === lapId)) return prev;
      return [...prev, meta];
    });
    setDraftIds((prev) => toggleSelectedId(prev, lapId));
  }

  function handleCompare() {
    if (draftIds.length === 0) return;
    setCompareIds(draftIds);
    setReferenceId((prev) =>
      prev && draftIds.includes(prev) ? prev : draftIds[0],
    );
    setPickerOpen(false);
  }

  function handleReopenPicker() {
    setDraftIds(compareIds);
    setPickerOpen(true);
  }

  return (
    <div className="page compare-page">
      <div className="page-inner">
        <header className="page-header compare-page-header">
          <div>
            <Link
              to={sessionId ? `/sessions/${sessionId}` : "/"}
              className="back-link"
            >
              ← Review
            </Link>
            <h1>Lap Comparison</h1>
          </div>
        </header>
      </div>

      <div className="compare-view-root">
        <LapCompareView
        mode="session"
        catalog={catalog}
        selectedIds={compareIds}
        referenceId={referenceId}
        samples={samples}
        game={sessionMeta?.game ?? null}
        trackLayout={trackLayout}
        layoutLoading={layoutLoading}
        error={error}
        lapPanel={
          <LapPanel
            laps={laps}
            draftIds={draftIds}
            comparedIds={compareIds}
            pickerOpen={pickerOpen}
            referenceId={referenceId}
            externalMetas={externalMetas}
            canAddExternal={canAddLap(draftIds.length) && sessionMeta != null}
            onToggleLap={toggleLap}
            onSetReference={setReferenceId}
            onPinLap={pinLap}
            onLapsRefresh={() => {
              if (sessionId) listLaps(sessionId).then(setLaps);
            }}
            onAddExternal={openModal}
            onCompare={handleCompare}
            onReopenPicker={handleReopenPicker}
          />
        }
        />
      </div>

      <AddExternalLapModal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        laps={trackLaps}
        loading={trackLapsLoading}
        selectedIds={draftIds}
        currentSessionId={sessionId ?? ""}
        onSelect={addExternalLap}
      />
    </div>
  );
}
