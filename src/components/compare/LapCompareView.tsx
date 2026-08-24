import { useCallback, useEffect, useMemo, useRef, useState, memo, type CSSProperties, type RefObject, type ReactNode } from "react";
import type uPlot from "uplot";
import { DistanceChart } from "../charts/DistanceChart";
import { SegmentDeltaStrip } from "../charts/SegmentDeltaStrip";
import { TrackMap } from "../charts/TrackMap";
import {
  CollapsibleChart,
  CollapsiblePanel,
  ResizeSplitter,
} from "./CompareLayout";
import {
  filterSamplesToRange,
  getActiveSegmentRange,
  getSegmentRanges,
  mapCursorToRangeLocal,
  mapRangeLocalToLapPct,
  computeSegmentDeltas,
  computeSegmentTimes,
  type SegmentTab,
} from "../../lib/segments";
import {
  buildFuelUsedSamples,
  lapHasChannel,
  TYRE_CORNER_LABELS,
  TYRE_PRESS_CHANNELS,
  TYRE_PRESS_LABELS,
  TYRE_TEMP_CHANNELS,
} from "../../lib/accExtras";
import { buildTimeDeltaSeries, applyCursorToPlot } from "../../lib/chartAlign";
import {
  collectDisplayValues,
  yRangeForChannel,
  type ChartYRange,
} from "../../lib/chartYScale";
import type { TrackLayout } from "../../lib/trackLayout";
import {
  type CompareLapMeta,
  type CompareMode,
  formatCompareDeltaLabel,
  formatCompareLapLabel,
} from "../../lib/compareLaps";
import {
  DEFAULT_CHART_HEIGHT,
  getLapColor,
  usePreferences,
} from "../../lib/preferences";
import type { DistanceSample, DistanceSampleChannel, GameId } from "../../types";
import { formatLapTime } from "../../types";

const CHANNEL_KEYS = [
  "speed_mps",
  "throttle",
  "brake",
  "steering",
  "gear",
  "rpm",
] as const;

const CHANNEL_LABELS: Record<string, string> = {
  speed_mps: "Speed",
  throttle: "Throttle",
  brake: "Brake",
  steering: "Steering",
  gear: "Gear",
  rpm: "RPM",
  lap_time_s: "Time Delta",
};

export interface LapCompareViewProps {
  mode: CompareMode;
  catalog: CompareLapMeta[];
  selectedIds: string[];
  referenceId: string | null;
  samples: Record<string, DistanceSample[]>;
  game?: GameId | null;
  trackLayout: TrackLayout | null;
  layoutLoading: boolean;
  error?: string | null;
  lapPanel?: ReactNode;
}

const TYRE_CHART_HEIGHT = 160;

interface CompareChartsColumnProps {
  selectedIds: string[];
  game?: GameId | null;
  deltaSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  chartSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  tyreSeriesByChannel: Record<
    (typeof TYRE_TEMP_CHANNELS)[number] | (typeof TYRE_PRESS_CHANNELS)[number],
    Array<{ label: string; color: string; samples: DistanceSample[] }>
  >;
  tyreNoDataByChannel: Record<
    (typeof TYRE_TEMP_CHANNELS)[number] | (typeof TYRE_PRESS_CHANNELS)[number],
    boolean
  >;
  fuelUsedSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  fuelShowNoData: boolean;
  scaleChartSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  scaleDeltaSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  scaleFuelUsedSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  tyreTempYRange: ChartYRange;
  tyrePressYRange: ChartYRange;
  segmentZoom: boolean;
  chartCollapsed: Record<string, boolean>;
  chartHeights: Record<string, number>;
  scrollRef: RefObject<HTMLDivElement | null>;
  onToggleChart: (key: string) => void;
  onChartResizeCommit: (key: string, height: number) => void;
  onChartCursorMove: (localPct: number | null) => void;
  onPlotMount: (plot: uPlot) => void;
  onPlotUnmount: (plot: uPlot) => void;
}

const CompareChartsColumn = memo(function CompareChartsColumn({
  selectedIds,
  game,
  deltaSeries,
  chartSeries,
  tyreSeriesByChannel,
  tyreNoDataByChannel,
  fuelUsedSeries,
  fuelShowNoData,
  scaleChartSeries,
  scaleDeltaSeries,
  scaleFuelUsedSeries,
  tyreTempYRange,
  tyrePressYRange,
  segmentZoom,
  chartCollapsed,
  chartHeights,
  scrollRef,
  onToggleChart,
  onChartResizeCommit,
  onChartCursorMove,
  onPlotMount,
  onPlotUnmount,
}: CompareChartsColumnProps) {
  const renderTyreGroup = (
    groupKey: string,
    keys: readonly DistanceSampleChannel[],
    labels: Record<string, string>,
    sectionTitle: string,
    yRange: ChartYRange,
  ) => (
    <CollapsibleChart
      title={sectionTitle}
      collapsed={chartCollapsed[groupKey] ?? false}
      height={chartHeights[groupKey] ?? TYRE_CHART_HEIGHT}
      onToggle={() => onToggleChart(groupKey)}
      onResizeCommit={(h) => onChartResizeCommit(groupKey, h)}
    >
      {(displayHeight) => (
        <div className="compare-tyre-grid">
          <DistanceChart
            title={"\u00a0"}
            channelKey={keys[0]}
            game={game}
            samplesByLap={[]}
            yRange={yRange}
            compact
            yAxisOnly
            height={displayHeight}
          />
          {keys.map((key) => (
            <DistanceChart
              key={key}
              title={labels[key] ?? key}
              channelKey={key}
              game={game}
              samplesByLap={tyreSeriesByChannel[key as keyof typeof tyreSeriesByChannel] ?? []}
              showNoData={tyreNoDataByChannel[key as keyof typeof tyreNoDataByChannel] ?? false}
              yRange={yRange}
              onCursorMove={onChartCursorMove}
              onPlotMount={onPlotMount}
              onPlotUnmount={onPlotUnmount}
              segmentZoom={segmentZoom}
              compact
              hideYAxis
              height={displayHeight}
            />
          ))}
        </div>
      )}
    </CollapsibleChart>
  );

  return (
    <div className="compare-charts-scroll" ref={scrollRef}>
      {selectedIds.length === 0 && (
        <p className="muted chart-empty">
          Select laps and press Compare to view charts.
        </p>
      )}

      {deltaSeries.length > 0 &&
        deltaSeries.some((s) => s.samples.length > 0) && (
        <CollapsibleChart
          title="Time Delta"
          collapsed={chartCollapsed.lap_time_s ?? false}
          height={chartHeights.lap_time_s ?? DEFAULT_CHART_HEIGHT}
          onToggle={() => onToggleChart("lap_time_s")}
          onResizeCommit={(h) => onChartResizeCommit("lap_time_s", h)}
        >
          {(displayHeight) => (
            <DistanceChart
              title="Time Delta"
              channelKey="lap_time_s"
              game={game}
              samplesByLap={deltaSeries}
              scaleSamplesByLap={scaleDeltaSeries}
              onCursorMove={onChartCursorMove}
              onPlotMount={onPlotMount}
              onPlotUnmount={onPlotUnmount}
              segmentZoom={segmentZoom}
              height={displayHeight}
            />
          )}
        </CollapsibleChart>
      )}

      {CHANNEL_KEYS.map((key) => (
        <CollapsibleChart
          key={key}
          title={CHANNEL_LABELS[key]}
          collapsed={chartCollapsed[key] ?? false}
          height={chartHeights[key] ?? DEFAULT_CHART_HEIGHT}
          onToggle={() => onToggleChart(key)}
          onResizeCommit={(h) => onChartResizeCommit(key, h)}
        >
          {(displayHeight) => (
            <DistanceChart
              title={CHANNEL_LABELS[key]}
              channelKey={key}
              game={game}
              samplesByLap={chartSeries}
              scaleSamplesByLap={scaleChartSeries}
              onCursorMove={onChartCursorMove}
              onPlotMount={onPlotMount}
              onPlotUnmount={onPlotUnmount}
              segmentZoom={segmentZoom}
              height={displayHeight}
            />
          )}
        </CollapsibleChart>
      ))}

      {renderTyreGroup(
        "tyre_temps",
        TYRE_TEMP_CHANNELS,
        TYRE_CORNER_LABELS,
        "Tyre core temps",
        tyreTempYRange,
      )}
      {renderTyreGroup(
        "tyre_pressures",
        TYRE_PRESS_CHANNELS,
        TYRE_PRESS_LABELS,
        "Tyre pressures",
        tyrePressYRange,
      )}

      <CollapsibleChart
        title="Fuel used (cumulative)"
        collapsed={chartCollapsed.fuel_used ?? true}
        height={chartHeights.fuel_used ?? DEFAULT_CHART_HEIGHT}
        onToggle={() => onToggleChart("fuel_used")}
        onResizeCommit={(h) => onChartResizeCommit("fuel_used", h)}
      >
        {(displayHeight) => (
          <DistanceChart
            title="Fuel used (cumulative)"
            channelKey="fuel"
            game={game}
            samplesByLap={fuelUsedSeries}
            scaleSamplesByLap={scaleFuelUsedSeries}
            showNoData={fuelShowNoData}
            onCursorMove={onChartCursorMove}
            onPlotMount={onPlotMount}
            onPlotUnmount={onPlotUnmount}
            segmentZoom={segmentZoom}
            height={displayHeight}
          />
        )}
      </CollapsibleChart>
    </div>
  );
});

export function LapCompareView({
  mode,
  catalog,
  selectedIds,
  referenceId,
  samples,
  game = null,
  trackLayout,
  layoutLoading,
  error,
  lapPanel,
}: LapCompareViewProps) {
  const [prefs, setPrefs] = usePreferences();
  const [trackCursorPct, setTrackCursorPct] = useState<number | null>(null);
  const [trackMode, setTrackMode] = useState<"speed" | "delta">("speed");
  const [segmentTab, setSegmentTab] = useState<SegmentTab>("full");
  const [liveColumnSplitPct, setLiveColumnSplitPct] = useState<number | null>(null);

  const chartPlotsRef = useRef<Set<uPlot>>(new Set());
  const gridRef = useRef<HTMLDivElement>(null);
  const chartsScrollRef = useRef<HTMLDivElement>(null);
  const chartsScrollTopRef = useRef(0);
  const rightColRef = useRef<HTMLElement>(null);
  const liveMapSplitRef = useRef<number | null>(null);
  const segmentTabRef = useRef(segmentTab);
  segmentTabRef.current = segmentTab;

  const segmentRanges = useMemo(() => getSegmentRanges(), []);
  const activeSegmentRange = getActiveSegmentRange(segmentTab, segmentRanges);
  const segmentZoom = segmentTab !== "full";

  const rememberChartsScroll = useCallback(() => {
    if (chartsScrollRef.current) {
      chartsScrollTopRef.current = chartsScrollRef.current.scrollTop;
    }
  }, []);

  const restoreChartsScroll = useCallback(() => {
    const top = chartsScrollTopRef.current;
    const apply = () => {
      if (chartsScrollRef.current) {
        chartsScrollRef.current.scrollTop = top;
      }
    };
    requestAnimationFrame(() => {
      apply();
      requestAnimationFrame(apply);
    });
  }, []);

  useEffect(() => {
    const el = chartsScrollRef.current;
    if (!el) return;
    const onScroll = () => {
      chartsScrollTopRef.current = el.scrollTop;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  const lapColorForId = useCallback(
    (lapId: string) => {
      const idx = selectedIds.indexOf(lapId);
      return getLapColor(idx >= 0 ? idx : 0);
    },
    [selectedIds, prefs.appearance.lapColors],
  );

  const columnSplitPct = liveColumnSplitPct ?? prefs.layout.columnSplitPct;
  const mapLapSplitPct = liveMapSplitRef.current ?? prefs.layout.mapLapSplitPct;

  useEffect(() => {
    if (rightColRef.current) {
      rightColRef.current.style.setProperty(
        "--map-split-pct",
        `${prefs.layout.mapLapSplitPct}%`,
      );
    }
  }, [prefs.layout.mapLapSplitPct]);

  const selectedMetas = useMemo(
    () =>
      selectedIds
        .map((id) => catalog.find((m) => m.lapId === id))
        .filter((m): m is CompareLapMeta => m != null),
    [catalog, selectedIds],
  );

  const metasWithSamples = useMemo(
    () =>
      selectedMetas.filter((meta) => (samples[meta.lapId]?.length ?? 0) > 0),
    [selectedMetas, samples],
  );

  const chartSeries = useMemo(() => {
    return metasWithSamples.map((meta) => {
      let lapSamples = samples[meta.lapId] ?? [];
      if (activeSegmentRange) {
        lapSamples = filterSamplesToRange(lapSamples, activeSegmentRange);
      }
      return {
        label: formatCompareLapLabel(meta, mode),
        color: lapColorForId(meta.lapId),
        samples: lapSamples,
      };
    });
  }, [metasWithSamples, samples, activeSegmentRange, mode, lapColorForId]);

  const fullChartSeries = useMemo(
    () =>
      metasWithSamples.map((meta) => ({
        label: formatCompareLapLabel(meta, mode),
        color: lapColorForId(meta.lapId),
        samples: samples[meta.lapId] ?? [],
      })),
    [metasWithSamples, samples, mode, lapColorForId],
  );

  const referenceSamples = referenceId ? samples[referenceId] ?? [] : [];

  const deltaSeries = useMemo(() => {
    if (referenceSamples.length === 0 || metasWithSamples.length <= 1) return [];
    return metasWithSamples
      .filter((meta) => meta.lapId !== referenceId)
      .map((meta) => ({
        label: formatCompareDeltaLabel(meta, mode),
        color: lapColorForId(meta.lapId),
        samples: buildTimeDeltaSeries(
          samples[meta.lapId] ?? [],
          referenceSamples,
          activeSegmentRange,
        ),
      }));
  }, [
    referenceSamples,
    metasWithSamples,
    referenceId,
    samples,
    activeSegmentRange,
    mode,
    lapColorForId,
  ]);

  const buildChannelSeries = useCallback(
    (channelKey: DistanceSampleChannel) =>
      metasWithSamples
        .filter((meta) => lapHasChannel(samples[meta.lapId] ?? [], channelKey))
        .map((meta) => {
          let lapSamples = samples[meta.lapId] ?? [];
          if (activeSegmentRange) {
            lapSamples = filterSamplesToRange(lapSamples, activeSegmentRange);
          }
          return {
            label: formatCompareLapLabel(meta, mode),
            color: lapColorForId(meta.lapId),
            samples: lapSamples,
          };
        }),
    [metasWithSamples, samples, activeSegmentRange, mode, lapColorForId],
  );

  const tyreSeriesByChannel = useMemo(() => {
    const channels = [...TYRE_TEMP_CHANNELS, ...TYRE_PRESS_CHANNELS] as const;
    return Object.fromEntries(
      channels.map((key) => [key, buildChannelSeries(key)]),
    ) as CompareChartsColumnProps["tyreSeriesByChannel"];
  }, [buildChannelSeries]);

  const tyreNoDataByChannel = useMemo(() => {
    const channels = [...TYRE_TEMP_CHANNELS, ...TYRE_PRESS_CHANNELS] as const;
    return Object.fromEntries(
      channels.map((key) => [
        key,
        metasWithSamples.length > 0 &&
          !metasWithSamples.some((meta) =>
            lapHasChannel(samples[meta.lapId] ?? [], key),
          ),
      ]),
    ) as CompareChartsColumnProps["tyreNoDataByChannel"];
  }, [metasWithSamples, samples]);

  const fuelUsedSeries = useMemo(
    () =>
      metasWithSamples
        .filter((meta) => lapHasChannel(samples[meta.lapId] ?? [], "fuel"))
        .map((meta) => {
          let lapSamples = buildFuelUsedSamples(samples[meta.lapId] ?? []);
          if (activeSegmentRange) {
            lapSamples = filterSamplesToRange(lapSamples, activeSegmentRange);
          }
          return {
            label: formatCompareLapLabel(meta, mode),
            color: lapColorForId(meta.lapId),
            samples: lapSamples,
          };
        }),
    [metasWithSamples, samples, activeSegmentRange, mode, lapColorForId],
  );

  const fuelShowNoData =
    metasWithSamples.length > 0 &&
    !metasWithSamples.some((meta) =>
      lapHasChannel(samples[meta.lapId] ?? [], "fuel"),
    );

  const scaleDeltaSeries = useMemo(() => {
    if (referenceSamples.length === 0 || metasWithSamples.length <= 1) return [];
    return metasWithSamples
      .filter((meta) => meta.lapId !== referenceId)
      .map((meta) => ({
        label: formatCompareDeltaLabel(meta, mode),
        color: lapColorForId(meta.lapId),
        samples: buildTimeDeltaSeries(
          samples[meta.lapId] ?? [],
          referenceSamples,
        ),
      }));
  }, [
    referenceSamples,
    metasWithSamples,
    referenceId,
    samples,
    mode,
    lapColorForId,
  ]);

  const scaleFuelUsedSeries = useMemo(
    () =>
      metasWithSamples
        .filter((meta) => lapHasChannel(samples[meta.lapId] ?? [], "fuel"))
        .map((meta) => ({
          label: formatCompareLapLabel(meta, mode),
          color: lapColorForId(meta.lapId),
          samples: buildFuelUsedSamples(samples[meta.lapId] ?? []),
        })),
    [metasWithSamples, samples, mode, lapColorForId],
  );

  const fullLapSampleLists = useMemo(
    () => metasWithSamples.map((meta) => samples[meta.lapId] ?? []),
    [metasWithSamples, samples],
  );

  const tyreTempYRange = useMemo(() => {
    const values = TYRE_TEMP_CHANNELS.flatMap((key) =>
      collectDisplayValues(fullLapSampleLists, key, prefs),
    );
    return yRangeForChannel("tyre_temp_fl", values);
  }, [fullLapSampleLists, prefs.tempUnit]);

  const tyrePressYRange = useMemo(() => {
    const values = TYRE_PRESS_CHANNELS.flatMap((key) =>
      collectDisplayValues(fullLapSampleLists, key, prefs),
    );
    return yRangeForChannel("tyre_press_fl", values);
  }, [fullLapSampleLists, prefs.pressureUnit]);

  useEffect(() => {
    restoreChartsScroll();
  }, [segmentTab, chartSeries, deltaSeries, restoreChartsScroll]);

  const registerChartPlot = useCallback((plot: uPlot) => {
    chartPlotsRef.current.add(plot);
  }, []);

  const unregisterChartPlot = useCallback((plot: uPlot) => {
    chartPlotsRef.current.delete(plot);
  }, []);

  const applyChartsCursor = useCallback(
    (lapPct: number | null) => {
      const localPct = mapCursorToRangeLocal(
        lapPct,
        segmentTabRef.current,
        segmentRanges,
      );
      for (const plot of chartPlotsRef.current) {
        applyCursorToPlot(plot, localPct);
      }
    },
    [segmentRanges],
  );

  const handleChartCursorMove = useCallback(
    (localPct: number | null) => {
      const lapPct = mapRangeLocalToLapPct(
        localPct,
        segmentTabRef.current,
        segmentRanges,
      );
      applyChartsCursor(lapPct);
      setTrackCursorPct((prev) => {
        if (lapPct == null && prev == null) return prev;
        if (
          lapPct != null &&
          prev != null &&
          Math.abs(lapPct - prev) < 0.0001
        ) {
          return prev;
        }
        return lapPct;
      });
    },
    [segmentRanges, applyChartsCursor],
  );

  const handleTrackMapCursorMove = useCallback(
    (pct: number | null) => {
      setTrackCursorPct(pct);
      applyChartsCursor(pct);
    },
    [applyChartsCursor],
  );

  useEffect(() => {
    if (trackCursorPct != null) {
      applyChartsCursor(trackCursorPct);
    }
  }, [segmentTab, segmentRanges, applyChartsCursor]);

  const handleColumnDrag = useCallback(
    (deltaPx: number) => {
      if (!gridRef.current) return;
      const total = gridRef.current.clientWidth;
      if (total <= 0) return;
      const deltaPct = (deltaPx / total) * 100;
      const base = liveColumnSplitPct ?? prefs.layout.columnSplitPct;
      const next = Math.min(85, Math.max(35, base + deltaPct));
      setLiveColumnSplitPct(next);
    },
    [liveColumnSplitPct, prefs.layout.columnSplitPct],
  );

  const handleColumnDragEnd = useCallback(() => {
    if (liveColumnSplitPct == null) return;
    setPrefs({ layout: { columnSplitPct: liveColumnSplitPct } });
    setLiveColumnSplitPct(null);
  }, [liveColumnSplitPct, setPrefs]);

  const handleMapLapDrag = useCallback(
    (deltaPx: number) => {
      const el = rightColRef.current;
      if (!el) return;
      const total = el.clientHeight;
      if (total <= 0) return;
      const deltaPct = (deltaPx / total) * 100;
      const base = liveMapSplitRef.current ?? prefs.layout.mapLapSplitPct;
      const next = Math.min(85, Math.max(25, base + deltaPct));
      liveMapSplitRef.current = next;
      el.style.setProperty("--map-split-pct", `${next}%`);
    },
    [prefs.layout.mapLapSplitPct],
  );

  const handleMapLapDragEnd = useCallback(() => {
    const next = liveMapSplitRef.current;
    if (next == null) return;
    setPrefs({ layout: { mapLapSplitPct: next } });
    liveMapSplitRef.current = null;
  }, [setPrefs]);

  const handleChartResizeCommit = useCallback(
    (key: string, height: number) => {
      const scrollTop = chartsScrollRef.current?.scrollTop ?? 0;
      setPrefs({
        layout: {
          chartHeights: { ...prefs.layout.chartHeights, [key]: height },
        },
      });
      requestAnimationFrame(() => {
        if (chartsScrollRef.current) {
          chartsScrollRef.current.scrollTop = scrollTop;
        }
      });
    },
    [prefs.layout.chartHeights, setPrefs],
  );

  const toggleChartCollapsed = useCallback(
    (key: string) => {
      const scrollTop = chartsScrollRef.current?.scrollTop ?? 0;
      const current = prefs.layout.chartCollapsed[key] ?? false;
      setPrefs({
        layout: {
          chartCollapsed: { ...prefs.layout.chartCollapsed, [key]: !current },
        },
      });
      requestAnimationFrame(() => {
        if (chartsScrollRef.current) {
          chartsScrollRef.current.scrollTop = scrollTop;
        }
      });
    },
    [prefs.layout.chartCollapsed, setPrefs],
  );

  const compareLaps = selectedMetas.filter((m) => m.lapId !== referenceId);

  const segmentStripRows = useMemo(() => {
    if (metasWithSamples.length === 0) return [];

    if (compareLaps.length > 0 && referenceSamples.length > 0) {
      return compareLaps.map((meta) => ({
        key: meta.lapId,
        label: formatCompareLapLabel(meta, mode),
        lapColor: lapColorForId(meta.lapId),
        fullLapTime: formatLapTime(meta.lapTimeMs),
        mode: "delta" as const,
        values: computeSegmentDeltas(
          referenceSamples,
          samples[meta.lapId] ?? [],
        ),
      }));
    }

    if (metasWithSamples.length === 1) {
      const meta = metasWithSamples[0];
      const lapSamples = samples[meta.lapId] ?? [];
      if (lapSamples.length === 0) return [];
      return [{
        key: meta.lapId,
        label: formatCompareLapLabel(meta, mode),
        lapColor: lapColorForId(meta.lapId),
        fullLapTime: formatLapTime(meta.lapTimeMs),
        mode: "time" as const,
        values: computeSegmentTimes(lapSamples),
      }];
    }

    return [];
  }, [compareLaps, metasWithSamples, referenceSamples, samples, mode, lapColorForId]);

  return (
    <>
      {error && <p className="error">{error}</p>}

      <div className="compare-page-grid" ref={gridRef}>
        <div
          className="compare-left"
          style={{ width: `${columnSplitPct}%` }}
        >
          <div className="compare-sticky-header">
            {segmentStripRows.length > 0 && (
              <div className="compare-segment-deltas">
                {segmentStripRows.map((row) => (
                  <div key={row.key} className="segment-row compact">
                    <span>{row.label}</span>
                    <SegmentDeltaStrip
                      values={row.values}
                      mode={row.mode}
                      lapColor={row.lapColor}
                      fullLapTime={row.fullLapTime}
                      selectedSegment={
                        segmentTab === "full" ? null : segmentTab
                      }
                      onSelectSegment={(seg) => {
                        rememberChartsScroll();
                        if (seg == null) setSegmentTab("full");
                        else setSegmentTab(seg);
                      }}
                    />
                  </div>
                ))}
              </div>
            )}
          </div>

          <CompareChartsColumn
            selectedIds={selectedIds}
            game={game}
            deltaSeries={deltaSeries}
            chartSeries={chartSeries}
            tyreSeriesByChannel={tyreSeriesByChannel}
            tyreNoDataByChannel={tyreNoDataByChannel}
            fuelUsedSeries={fuelUsedSeries}
            fuelShowNoData={fuelShowNoData}
            scaleChartSeries={fullChartSeries}
            scaleDeltaSeries={scaleDeltaSeries}
            scaleFuelUsedSeries={scaleFuelUsedSeries}
            tyreTempYRange={tyreTempYRange}
            tyrePressYRange={tyrePressYRange}
            segmentZoom={segmentZoom}
            chartCollapsed={prefs.layout.chartCollapsed}
            chartHeights={prefs.layout.chartHeights}
            scrollRef={chartsScrollRef}
            onToggleChart={toggleChartCollapsed}
            onChartResizeCommit={handleChartResizeCommit}
            onChartCursorMove={handleChartCursorMove}
            onPlotMount={registerChartPlot}
            onPlotUnmount={unregisterChartPlot}
          />
        </div>

        <ResizeSplitter
          orientation="vertical"
          onDrag={handleColumnDrag}
          onDragEnd={handleColumnDragEnd}
        />

        <aside
          ref={rightColRef}
          className={`compare-right${
            prefs.layout.mapCollapsed ? " map-collapsed" : ""
          }${prefs.layout.lapsCollapsed ? " laps-collapsed" : ""}`}
          style={
            { "--map-split-pct": `${mapLapSplitPct}%` } as CSSProperties
          }
        >
          <div className="compare-right-map">
            <CollapsiblePanel
              title="Track Map"
              collapsed={prefs.layout.mapCollapsed}
              onToggle={() =>
                setPrefs({
                  layout: { mapCollapsed: !prefs.layout.mapCollapsed },
                })
              }
              toolbar={
                <div>
                  <button
                    type="button"
                    className={trackMode === "speed" ? "" : "secondary"}
                    onClick={() => setTrackMode("speed")}
                  >
                    Speed
                  </button>
                  <button
                    type="button"
                    className={trackMode === "delta" ? "" : "secondary"}
                    onClick={() => setTrackMode("delta")}
                  >
                    Delta
                  </button>
                </div>
              }
              className="track-map-panel"
            >
              <TrackMap
                layout={trackLayout}
                layoutLoading={layoutLoading}
                samplesByLap={fullChartSeries}
                mode={trackMode}
                reference={referenceSamples}
                cursorPct={trackCursorPct}
                onCursorMove={handleTrackMapCursorMove}
              />
            </CollapsiblePanel>
          </div>

          {!prefs.layout.mapCollapsed && lapPanel && (
            <ResizeSplitter
              orientation="horizontal"
              onDrag={handleMapLapDrag}
              onDragEnd={handleMapLapDragEnd}
            />
          )}

          {lapPanel && (
            <div className="compare-right-laps">
              <CollapsiblePanel
                title="Laps"
                collapsed={prefs.layout.lapsCollapsed}
                onToggle={() =>
                  setPrefs({
                    layout: { lapsCollapsed: !prefs.layout.lapsCollapsed },
                  })
                }
                className="lap-panel-wrapper"
              >
                {lapPanel}
              </CollapsiblePanel>
            </div>
          )}
        </aside>
      </div>
    </>
  );
}
