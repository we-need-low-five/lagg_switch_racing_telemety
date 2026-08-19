import { useMemo, useState } from "react";
import {
  computeFuelPlan,
  formatFuelLiters,
} from "../lib/fuelCalc";

function parseOptionalInt(value: string): number {
  if (value === "") return 0;
  const n = Number(value);
  return Number.isFinite(n) ? n : NaN;
}

export function FuelCalculator() {
  const [hours, setHours] = useState("");
  const [minutes, setMinutes] = useState("");
  const [lapMinutes, setLapMinutes] = useState("");
  const [lapSeconds, setLapSeconds] = useState("");
  const [lapMilliseconds, setLapMilliseconds] = useState("");
  const [fuelPerLap, setFuelPerLap] = useState("");
  const [safetyMargin, setSafetyMargin] = useState(false);

  const result = useMemo(
    () =>
      computeFuelPlan({
        hours: hours === "" ? 0 : Number(hours),
        minutes: minutes === "" ? 0 : Number(minutes),
        lapMinutes: parseOptionalInt(lapMinutes),
        lapSeconds: parseOptionalInt(lapSeconds),
        lapMilliseconds: parseOptionalInt(lapMilliseconds),
        fuelPerLapL: fuelPerLap === "" ? NaN : Number(fuelPerLap),
        safetyMargin,
      }),
    [hours, minutes, lapMinutes, lapSeconds, lapMilliseconds, fuelPerLap, safetyMargin],
  );

  const hasLapTimeInput =
    lapMinutes !== "" || lapSeconds !== "" || lapMilliseconds !== "";

  const hasInput =
    hours !== "" ||
    minutes !== "" ||
    hasLapTimeInput ||
    fuelPerLap !== "";

  return (
    <div className="page">
      <div className="page-inner">
        <header className="page-header">
          <div>
            <h1>Fuel Calculator</h1>
            <p className="subtitle">
              Plan race fuel from duration, average pace, and consumption per lap.
            </p>
          </div>
        </header>

        <div className="fuel-calc-grid">
          <section className="settings-panel fuel-calc-form">
            <h2 className="fuel-calc-section-title">Race</h2>
            <div className="form-row">
              <label>
                <span className="form-label">Hours</span>
                <input
                  type="number"
                  min={0}
                  step={1}
                  placeholder="0"
                  value={hours}
                  onChange={(e) => setHours(e.target.value)}
                />
              </label>
              <label>
                <span className="form-label">Minutes</span>
                <input
                  type="number"
                  min={0}
                  step={1}
                  placeholder="0"
                  value={minutes}
                  onChange={(e) => setMinutes(e.target.value)}
                />
              </label>
            </div>

            <h2 className="fuel-calc-section-title">Pace & consumption</h2>
            <span className="form-label">Average lap time</span>
            <div className="form-row form-row-triple">
              <label>
                <span className="form-label">Minutes</span>
                <input
                  type="number"
                  min={0}
                  step={1}
                  placeholder="0"
                  value={lapMinutes}
                  onChange={(e) => setLapMinutes(e.target.value)}
                />
              </label>
              <label>
                <span className="form-label">Seconds</span>
                <input
                  type="number"
                  min={0}
                  max={59}
                  step={1}
                  placeholder="0"
                  value={lapSeconds}
                  onChange={(e) => setLapSeconds(e.target.value)}
                />
              </label>
              <label>
                <span className="form-label">Milliseconds</span>
                <input
                  type="number"
                  min={0}
                  max={999}
                  step={1}
                  placeholder="0"
                  value={lapMilliseconds}
                  onChange={(e) => setLapMilliseconds(e.target.value)}
                />
              </label>
            </div>
            <label className="form-field">
              <span className="form-label">Fuel per lap (L)</span>
              <input
                type="number"
                min={0}
                step={0.01}
                placeholder="2.5"
                value={fuelPerLap}
                onChange={(e) => setFuelPerLap(e.target.value)}
              />
            </label>

            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={safetyMargin}
                onChange={(e) => setSafetyMargin(e.target.checked)}
              />
              <span>Add safety margin (+2.5 laps fuel)</span>
            </label>
          </section>

          <section className="settings-panel fuel-calc-results">
            <h2 className="fuel-calc-section-title">Results</h2>
            {!hasInput && (
              <p className="muted">Enter race duration, lap time, and fuel per lap.</p>
            )}
            {hasInput && result == null && (
              <p className="error">
                Check lap time (seconds 0–59, ms 0–999) and fuel per lap.
              </p>
            )}
            {result != null && (
              <dl className="fuel-result-list">
                <div className="fuel-result-row">
                  <dt>Laps (rounded up)</dt>
                  <dd>{result.laps}</dd>
                </div>
                <div className="fuel-result-row">
                  <dt>Base fuel</dt>
                  <dd>{formatFuelLiters(result.baseFuelL)}</dd>
                </div>
                {safetyMargin && (
                  <div className="fuel-result-row">
                    <dt>Safety margin</dt>
                    <dd>{formatFuelLiters(result.marginFuelL)}</dd>
                  </div>
                )}
                <div className="fuel-result-row fuel-result-total">
                  <dt>Total fuel</dt>
                  <dd>{formatFuelLiters(result.totalFuelL)}</dd>
                </div>
              </dl>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
