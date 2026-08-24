import uPlot, { type AlignedData, type Options } from "uplot";
import "uplot/dist/uPlot.min.css";
import { useEffect, useRef } from "react";
import {
  fuelUnitLabel,
  pressureUnitLabel,
  speedUnitLabel,
  tempUnitLabel,
  usePreferences,
} from "../../lib/preferences";
import { alignLapSamples } from "../../lib/chartAlign";
import {
  accSteeringIsLegacyNormalized,
  collectDisplayValues,
  steeringIsDegrees,
  transformChannelValue,
  yRangeForChannel,
  type ChartYRange,
} from "../../lib/chartYScale";
import type { DistanceSample, GameId } from "../../types";

export interface SeriesConfig {
  key: keyof DistanceSample;
  label: string;
  color: string;
}

interface DistanceChartProps {
  title: string;
  channelKey: keyof DistanceSample;
  game?: GameId | null;
  samplesByLap: Array<{ label: string; color: string; samples: DistanceSample[] }>;
  onCursorMove?: (pct: number | null) => void;
  onPlotMount?: (plot: uPlot) => void;
  onPlotUnmount?: (plot: uPlot) => void;
  height?: number;
  segmentZoom?: boolean;
  compact?: boolean;
  /** Hide Y ticks, values, and axis title (grid stays). Used for grouped tyre plots. */
  hideYAxis?: boolean;
  /** Shared group scale only: ticks/label, no series. */
  yAxisOnly?: boolean;
  showNoData?: boolean;
  valueSelector?: (sample: DistanceSample) => number | null | undefined;
  /** Full-lap (or other) samples used to lock Y domain; defaults to `samplesByLap`. */
  scaleSamplesByLap?: Array<{ samples: DistanceSample[] }>;
  /** Overrides computed channel domain (e.g. shared tyre-grid scale). */
  yRange?: ChartYRange;
}

function usesTwoDecimalFormat(key: keyof DistanceSample): boolean {
  return (
    key === "fuel" ||
    key === "tyre_press_fl" ||
    key === "tyre_press_fr" ||
    key === "tyre_press_rl" ||
    key === "tyre_press_rr"
  );
}

function formatYTick(
  key: keyof DistanceSample,
  val: number,
  deltaUnit: "s" | "ms",
  game?: GameId | null,
): string {
  if (key === "lap_time_s") {
    if (deltaUnit === "ms") {
      return String(Math.round(val * 1000));
    }
    return val.toFixed(2);
  }
  if (key === "steering") {
    return formatSteeringReadout(val, game);
  }
  if (key === "gear" || key === "rpm") {
    return String(Math.round(val));
  }
  if (usesTwoDecimalFormat(key)) {
    return val.toFixed(2);
  }
  return String(Math.round(val));
}

function formatSteeringReadout(
  val: number | null,
  game?: GameId | null,
): string {
  if (val == null || !Number.isFinite(val)) return "";
  const rounded = Math.round(val);
  if (steeringIsDegrees(game)) {
    return `${rounded}°`;
  }
  if (rounded === 0) return "0%";
  const pct = Math.abs(rounded);
  return rounded < 0 ? `L ${pct}%` : `R ${pct}%`;
}

function formatLegendValueBody(
  key: keyof DistanceSample,
  val: number | null,
  dataIdx: number | null | undefined,
  deltaUnit: "s" | "ms",
  game?: GameId | null,
): string {
  if (dataIdx == null) return "--";
  if (val == null) return "";
  if (key === "steering") {
    return formatSteeringReadout(val, game);
  }
  if (key === "lap_time_s") {
    return formatYTick(key, val, deltaUnit);
  }
  if (usesTwoDecimalFormat(key)) {
    return val.toFixed(2);
  }
  return String(Math.round(val));
}

function legendValueSuffix(
  key: keyof DistanceSample,
  prefs: ReturnType<typeof usePreferences>[0],
): string | null {
  switch (key) {
    case "speed_mps":
      return speedUnitLabel(prefs.speedUnit);
    case "throttle":
    case "brake":
      return "%";
    case "gear":
      return "Gear";
    case "rpm":
      return "RPM";
    case "lap_time_s":
      return prefs.deltaUnit === "ms" ? "ms" : "s";
    case "fuel":
      return fuelUnitLabel(prefs.fuelUnit);
    case "tyre_temp_fl":
    case "tyre_temp_fr":
    case "tyre_temp_rl":
    case "tyre_temp_rr":
      return tempUnitLabel(prefs.tempUnit);
    case "tyre_press_fl":
    case "tyre_press_fr":
    case "tyre_press_rl":
    case "tyre_press_rr":
      return pressureUnitLabel(prefs.pressureUnit);
    case "steering":
      return null;
    default:
      return null;
  }
}

function formatLegendValueWithSuffix(
  key: keyof DistanceSample,
  val: number | null,
  dataIdx: number | null | undefined,
  prefs: ReturnType<typeof usePreferences>[0],
  game?: GameId | null,
): string {
  const body = formatLegendValueBody(key, val, dataIdx, prefs.deltaUnit, game);
  if (body === "--") return "--";
  if (body === "") return "";
  const suffix = legendValueSuffix(key, prefs);
  if (!suffix) return body;
  return `${body} ${suffix}`;
}

function yAxisLabel(
  key: keyof DistanceSample,
  prefs: ReturnType<typeof usePreferences>[0],
  game?: GameId | null,
): string {
  switch (key) {
    case "speed_mps":
      return `Speed (${speedUnitLabel(prefs.speedUnit)})`;
    case "throttle":
      return "Throttle (%)";
    case "brake":
      return "Brake (%)";
    case "steering":
      return steeringIsDegrees(game) ? "Steering (°)" : "Steering (L/R %)";
    case "gear":
      return "Gear";
    case "rpm":
      return "RPM";
    case "lap_time_s":
      return prefs.deltaUnit === "ms" ? "Delta (ms)" : "Delta (s)";
    case "fuel":
      return `Fuel used (${fuelUnitLabel(prefs.fuelUnit)})`;
    case "tyre_temp_fl":
    case "tyre_temp_fr":
    case "tyre_temp_rl":
    case "tyre_temp_rr":
      return `Temp (${tempUnitLabel(prefs.tempUnit)})`;
    case "tyre_press_fl":
    case "tyre_press_fr":
    case "tyre_press_rl":
    case "tyre_press_rr":
      return `Pressure (${pressureUnitLabel(prefs.pressureUnit)})`;
    default:
      return "";
  }
}

function getChartAxisColor(): string {
  return (
    getComputedStyle(document.documentElement)
      .getPropertyValue("--chart-axis")
      .trim() || "hsl(220 12% 62%)"
  );
}

function getChartGridColor(): string {
  return (
    getComputedStyle(document.documentElement)
      .getPropertyValue("--chart-grid")
      .trim() || "hsl(250 9% 18%)"
  );
}

function yAxisTickBandWidth(values: string[] | null): number {
  if (!values || values.length === 0) return 48;
  const maxLen = Math.max(...values.map((v) => v.length), 1);
  return Math.max(48, maxLen * 8 + 20);
}

function hideLegendDistanceRow(u: uPlot): void {
  const row = u.root.querySelector<HTMLElement>(
    ".u-legend tbody > tr.u-series:first-child",
  );
  if (row) row.style.setProperty("display", "none", "important");
}

export function DistanceChart({
  title,
  channelKey,
  game = null,
  samplesByLap,
  onCursorMove,
  onPlotMount,
  onPlotUnmount,
  height = 280,
  segmentZoom = false,
  compact = false,
  hideYAxis = false,
  yAxisOnly = false,
  showNoData = false,
  valueSelector,
  scaleSamplesByLap,
  yRange,
}: DistanceChartProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const onCursorMoveRef = useRef(onCursorMove);
  const onPlotMountRef = useRef(onPlotMount);
  const onPlotUnmountRef = useRef(onPlotUnmount);
  const heightRef = useRef(height);
  const [prefs] = usePreferences();
  const prefsRef = useRef(prefs);

  prefsRef.current = prefs;
  heightRef.current = height;
  onCursorMoveRef.current = onCursorMove;
  onPlotMountRef.current = onPlotMount;
  onPlotUnmountRef.current = onPlotUnmount;

  useEffect(() => {
    if (!rootRef.current || showNoData) return;
    if (!yAxisOnly && samplesByLap.length === 0) return;

    let data: AlignedData;
    const series: Options["series"] = [{}];

    if (yAxisOnly) {
      data = [
        [0, 100],
        [0, 1],
      ];
      series.push({ show: false });
    } else {
    const { x, aligned } = alignLapSamples(samplesByLap.map((lap) => lap.samples));
    if (x.length === 0) {
      if (plotRef.current) {
        onPlotUnmountRef.current?.(plotRef.current);
        plotRef.current.destroy();
        plotRef.current = null;
      }
      return;
    }

    data = [x];

    for (let i = 0; i < samplesByLap.length; i += 1) {
      const lap = samplesByLap[i];
      const alignedSamples = aligned[i];
      if (alignedSamples.length !== x.length) continue;

      const accLegacySteering =
        channelKey === "steering" &&
        game === "acc" &&
        accSteeringIsLegacyNormalized(alignedSamples);

      data.push(
        alignedSamples.map((s) => {
          const raw = valueSelector ? valueSelector(s) : Number(s[channelKey]);
          if (raw == null || !Number.isFinite(raw)) return null;
          return transformChannelValue(
            channelKey,
            raw,
            prefsRef.current,
            game,
            accLegacySteering,
          );
        }),
      );
      series.push({
        label: lap.label,
        stroke: lap.color,
        width: 2,
        points: {
          show: false,
          size: 8,
          width: 2,
          stroke: lap.color,
          fill: lap.color,
        },
        value: (_u, val, _si, dataIdx) =>
          formatLegendValueWithSuffix(
            channelKey,
            val,
            dataIdx,
            prefsRef.current,
            game,
          ),
      });
    }

      if (data.length < 2) {
        if (plotRef.current) {
          onPlotUnmountRef.current?.(plotRef.current);
          plotRef.current.destroy();
          plotRef.current = null;
        }
        return;
      }
    }

    const yLabel = yAxisLabel(channelKey, prefsRef.current, game);
    const axisColor = getChartAxisColor();
    const gridColor = getChartGridColor();
    const hideXAxis = segmentZoom || compact;
    const showYAxis = yAxisOnly || !hideYAxis;

    const scaleLists = (scaleSamplesByLap ?? samplesByLap).map(
      (lap) => lap.samples,
    );
    const domain =
      yRange ??
      yRangeForChannel(
        channelKey,
        collectDisplayValues(
          scaleLists,
          channelKey,
          prefsRef.current,
          game,
        ),
      );
    const yMin = domain.min;
    const yMax = domain.max;
    const plotHeight = heightRef.current;

    const opts: Options = {
      width: rootRef.current.clientWidth,
      height: plotHeight,
      title,
      padding: [8, 12, 4, 4],
      series,
      scales: {
        x: { time: false },
        y: {
          auto: false,
          range: () => [yMin, yMax],
        },
      },
      axes: [
        {
          stroke: axisColor,
          grid: { show: !yAxisOnly, stroke: gridColor },
          label: "",
          labelSize: 0,
          labelGap: 0,
          size: compact ? 36 : 50,
          ticks: { show: !hideXAxis },
          values: (_u, vals) =>
            hideXAxis
              ? vals.map(() => "")
              : vals.map((v) => `${Number(v).toFixed(0)}`),
        },
        {
          stroke: axisColor,
          grid: { show: !yAxisOnly, stroke: gridColor },
          label: showYAxis ? yLabel : "",
          labelSize: showYAxis ? 32 : 0,
          labelGap: showYAxis ? 8 : 0,
          gap: showYAxis ? 10 : 0,
          size: showYAxis
            ? (_u, values) => yAxisTickBandWidth(values)
            : 0,
          ticks: { show: showYAxis },
          ...(channelKey === "rpm" && {
            incrs: [100, 200, 250, 500, 1000, 2000, 2500, 5000],
          }),
          ...(channelKey === "gear" && {
            incrs: [1],
          }),
          values: (_u, vals) =>
            showYAxis
              ? vals.map((v) =>
                  formatYTick(channelKey, v, prefsRef.current.deltaUnit, game),
                )
              : vals.map(() => ""),
        },
      ],
      legend: {
        live: !yAxisOnly,
        show: true,
      },
      cursor: yAxisOnly
        ? { show: false }
        : {
            drag: { x: false, y: false },
            sync: { key: "simtelemetry" },
            focus: { prox: -1 },
            points: {
              width: 2,
            },
          },
      hooks: {
        ready: [hideLegendDistanceRow],
        setLegend: [hideLegendDistanceRow],
        setCursor: yAxisOnly
          ? []
          : [
              (u) => {
                if (!u.cursor.event) return;
                const idx = u.cursor.idx;
                if (idx == null) return;
                onCursorMoveRef.current?.(u.data[0][idx] ?? null);
              },
            ],
      },
    };

    if (plotRef.current) {
      if (!yAxisOnly) onPlotUnmountRef.current?.(plotRef.current);
      plotRef.current.destroy();
    }
    plotRef.current = new uPlot(opts, data, rootRef.current);
    if (!yAxisOnly) onPlotMountRef.current?.(plotRef.current);
    hideLegendDistanceRow(plotRef.current);

    const onWindowResize = () => {
      if (plotRef.current && rootRef.current) {
        plotRef.current.setSize({
          width: rootRef.current.clientWidth,
          height: heightRef.current,
        });
      }
    };
    window.addEventListener("resize", onWindowResize);
    return () => {
      window.removeEventListener("resize", onWindowResize);
      if (plotRef.current) {
        if (!yAxisOnly) onPlotUnmountRef.current?.(plotRef.current);
        plotRef.current.destroy();
        plotRef.current = null;
      }
    };
  }, [
    samplesByLap,
    channelKey,
    game,
    title,
    segmentZoom,
    compact,
    hideYAxis,
    yAxisOnly,
    showNoData,
    valueSelector,
    scaleSamplesByLap,
    yRange,
    prefs.speedUnit,
    prefs.deltaUnit,
    prefs.fuelUnit,
    prefs.tempUnit,
    prefs.pressureUnit,
    prefs.appearance.backgroundPreset,
    prefs.appearance.backgroundCustom,
  ]);

  useEffect(() => {
    if (!plotRef.current || !rootRef.current) return;
    plotRef.current.setSize({
      width: rootRef.current.clientWidth,
      height,
    });
  }, [height]);

  return (
    <div
      className={`chart-panel${compact ? " chart-panel-compact" : ""}${yAxisOnly ? " chart-panel-yaxis-only" : ""}`}
    >
      {showNoData ? (
        <div className="chart-no-data">
          <span className="muted">{title} — No data</span>
        </div>
      ) : !yAxisOnly &&
        (samplesByLap.length === 0 ||
          (samplesByLap[0]?.samples.length ?? 0) === 0) ? (
        <p className="muted small chart-empty">{title} — loading…</p>
      ) : null}
      <div className="chart-panel-plot" ref={rootRef} />
    </div>
  );
}

export const CHANNELS: SeriesConfig[] = [
  { key: "speed_mps", label: "Speed", color: "#38bdf8" },
  { key: "throttle", label: "Throttle", color: "#4ade80" },
  { key: "brake", label: "Brake", color: "#f87171" },
  { key: "steering", label: "Steering", color: "#a78bfa" },
  { key: "gear", label: "Gear", color: "#fbbf24" },
  { key: "rpm", label: "RPM", color: "#fb7185" },
  { key: "lap_time_s", label: "Time Delta", color: "#e2e8f0" },
];

export const LAP_COLORS = ["#38bdf8", "#f472b6", "#fbbf24", "#4ade80"];
