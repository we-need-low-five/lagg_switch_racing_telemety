import { useCallback, useEffect, useMemo, useRef, useState, memo, type RefObject, type ReactNode } from "react";
import type uPlot from "uplot";
import { DistanceChart } from "../charts/DistanceChart";
import { SegmentDeltaStrip } from "../charts/SegmentDeltaStrip";
import { TrackMap } from "../charts/TrackMap";
import { TractionCircle } from "../charts/TractionCircle";
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
  /** Called when the Track Map "Laps" toolbar button is pressed. */
  onOpenLapPicker?: () => void;
}

const TYRE_CHART_HEIGHT = 160;

type ExtraChannel =
  | (typeof TYRE_TEMP_CHANNELS)[number]
  | (typeof TYRE_PRESS_CHANNELS)[number];

const ALL_EXTRA_CHANNELS = [
  ...TYRE_TEMP_CHANNELS,
  ...TYRE_PRESS_CHANNELS,
] as const;

interface CompareChartsColumnProps {
  selectedIds: string[];
  game?: GameId | null;
  deltaSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  chartSeries: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  extraSeriesByChannel: Record<
    ExtraChannel,
    Array<{ label: string; color: string; samples: DistanceSample[] }>
  >;
  extraNoDataByChannel: Record<ExtraChannel, boolean>;
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
  extraSeriesByChannel,
  extraNoDataByChannel,
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
  const renderChannelGroup = (
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
              samplesByLap={extraSeriesByChannel[key as ExtraChannel] ?? []}
              showNoData={extraNoDataByChannel[key as ExtraChannel] ?? false}
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

      {renderChannelGroup(
        "tyre_temps",
        TYRE_TEMP_CHANNELS,
        TYRE_CORNER_LABELS,
        "Tyre core temps",
        tyreTempYRange,
      )}
      {renderChannelGroup(
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
  onOpenLapPicker,
}: LapCompareViewProps) {
  const [prefs, setPrefs] = usePreferences();
  const [trackCursorPct, setTrackCursorPct] = useState<number | null>(null);
  const [trackMode, setTrackMode] = useState<"speed" | "delta">("speed");
  const [showLaps, setShowLaps] = useState(false);
  const [segmentTab, setSegmentTab] = useState<SegmentTab>("full");
  const [liveColumnSplitPct, setLiveColumnSplitPct] = useState<number | null>(null);
  const [liveMapSplitPct, setLiveMapSplitPct] = useState<number | null>(null);
  const columnSplitPctRef = useRef(prefs.layout.columnSplitPct);
  const mapSplitPctRef = useRef(prefs.layout.mapLapSplitPct);

  const chartPlotsRef = useRef<Set<uPlot>>(new Set());
  const gridRef = useRef<HTMLDivElement>(null);
  const chartsScrollRef = useRef<HTMLDivElement>(null);
  const chartsScrollTopRef = useRef(0);
  const rightColRef = useRef<HTMLElement>(null);
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
  const mapSplitPct = liveMapSplitPct ?? prefs.layout.mapLapSplitPct;
  columnSplitPctRef.current = columnSplitPct;
  mapSplitPctRef.current = mapSplitPct;
  const mapExpanded = !prefs.layout.mapCollapsed;
  const tractionExpanded = !prefs.layout.tractionCircleCollapsed;
  const bothRightPanelsExpanded = mapExpanded && tractionExpanded;

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

  const extraSeriesByChannel = useMemo(() => {
    return Object.fromEntries(
      ALL_EXTRA_CHANNELS.map((key) => [key, buildChannelSeries(key)]),
    ) as CompareChartsColumnProps["extraSeriesByChannel"];
  }, [buildChannelSeries]);

  const extraNoDataByChannel = useMemo(() => {
    return Object.fromEntries(
      ALL_EXTRA_CHANNELS.map((key) => [
        key,
        metasWithSamples.length > 0 &&
          !metasWithSamples.some((meta) =>
            lapHasChannel(samples[meta.lapId] ?? [], key),
          ),
      ]),
    ) as CompareChartsColumnProps["extraNoDataByChannel"];
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

  const handleColumnDrag = useCallback((deltaPx: number) => {
    if (!gridRef.current) return;
    const total = gridRef.current.clientWidth;
    if (total <= 0) return;
    const next = Math.min(
      85,
      Math.max(35, columnSplitPctRef.current + (deltaPx / total) * 100),
    );
    columnSplitPctRef.current = next;
    setLiveColumnSplitPct(next);
  }, []);

  const handleColumnDragEnd = useCallback(() => {
    setPrefs({ layout: { columnSplitPct: columnSplitPctRef.current } });
    setLiveColumnSplitPct(null);
  }, [setPrefs]);

  const handleMapTractionDrag = useCallback((deltaPx: number) => {
    if (!rightColRef.current) return;
    const total = rightColRef.current.clientHeight;
    if (total <= 0) return;
    const next = Math.min(
      80,
      Math.max(20, mapSplitPctRef.current + (deltaPx / total) * 100),
    );
    mapSplitPctRef.current = next;
    setLiveMapSplitPct(next);
  }, []);

  const handleMapTractionDragEnd = useCallback(() => {
    setPrefs({ layout: { mapLapSplitPct: mapSplitPctRef.current } });
    setLiveMapSplitPct(null);
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
            extraSeriesByChannel={extraSeriesByChannel}
            extraNoDataByChannel={extraNoDataByChannel}
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
          }${
            prefs.layout.tractionCircleCollapsed
              ? " traction-collapsed"
              : ""
          }${bothRightPanelsExpanded ? " right-split" : ""}`}
        >
          <div
            className="compare-right-map"
            style={
              bothRightPanelsExpanded
                ? { flex: `0 0 ${mapSplitPct}%` }
                : undefined
            }
          >
            <CollapsiblePanel
              title="Track Map"
              collapsed={prefs.layout.mapCollapsed}
              onToggle={() =>
                setPrefs({
                  layout: { mapCollapsed: !prefs.layout.mapCollapsed },
                })
              }
              toolbar={
                <div className="track-map-toolbar">
                  <button
                    type="button"
                    className={
                      !showLaps && trackMode === "speed" ? "" : "secondary"
                    }
                    onClick={() => {
                      setShowLaps(false);
                      setTrackMode("speed");
                    }}
                  >
                    Speed
                  </button>
                  <button
                    type="button"
                    className={
                      !showLaps && trackMode === "delta" ? "" : "secondary"
                    }
                    onClick={() => {
                      setShowLaps(false);
                      setTrackMode("delta");
                    }}
                  >
                    Delta
                  </button>
                  {lapPanel && (
                    <button
                      type="button"
                      className={showLaps ? "" : "secondary"}
                      onClick={() => {
                        onOpenLapPicker?.();
                        setShowLaps(true);
                      }}
                    >
                      Laps
                    </button>
                  )}
                </div>
              }
              className="track-map-panel"
            >
              {showLaps && lapPanel ? (
                <div className="track-map-laps">{lapPanel}</div>
              ) : (
                <TrackMap
                  layout={trackLayout}
                  layoutLoading={layoutLoading}
                  samplesByLap={fullChartSeries}
                  mode={trackMode}
                  reference={referenceSamples}
                  cursorPct={trackCursorPct}
                  onCursorMove={handleTrackMapCursorMove}
                />
              )}
            </CollapsiblePanel>
          </div>

          {bothRightPanelsExpanded && (
            <ResizeSplitter
              orientation="horizontal"
              onDrag={handleMapTractionDrag}
              onDragEnd={handleMapTractionDragEnd}
            />
          )}

          <div
            className="compare-right-traction"
            style={
              bothRightPanelsExpanded ? { flex: "1 1 0" } : undefined
            }
          >
            <CollapsiblePanel
              title="Traction Circle"
              collapsed={prefs.layout.tractionCircleCollapsed}
              onToggle={() =>
                setPrefs({
                  layout: {
                    tractionCircleCollapsed:
                      !prefs.layout.tractionCircleCollapsed,
                  },
                })
              }
              className="traction-circle-panel"
            >
              <TractionCircle
                samplesByLap={chartSeries}
                cursorPct={trackCursorPct}
              />
            </CollapsiblePanel>
          </div>
        </aside>
      </div>
    </>
  );
}
