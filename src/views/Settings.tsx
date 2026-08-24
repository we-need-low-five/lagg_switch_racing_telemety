import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { getDataDir } from "../api";
import { GameSetupPanel } from "../components/GameSetupPanel";
import {
  BACKGROUND_PRESETS,
  type DeltaUnit,
  type FuelUnit,
  type PressureUnit,
  resetAppearance,
  type SpeedUnit,
  type TempUnit,
  usePreferences,
} from "../lib/preferences";

type SettingsTab = "setup" | "units" | "appearance" | "general";

const TABS: Array<{ id: SettingsTab; label: string }> = [
  { id: "setup", label: "Game Setup" },
  { id: "units", label: "Units" },
  { id: "appearance", label: "Appearance" },
  { id: "general", label: "General" },
];

export function Settings() {
  const [tab, setTab] = useState<SettingsTab>("setup");
  const [prefs, setPrefs] = usePreferences();
  const [dataDir, setDataDir] = useState("");

  useEffect(() => {
    getDataDir().then(setDataDir).catch(() => setDataDir(""));
  }, []);

  function updateLapColor(index: number, color: string) {
    const lapColors = [...prefs.appearance.lapColors];
    lapColors[index] = color;
    setPrefs({ appearance: { lapColors } });
  }

  return (
    <div className="page settings-page">
      <div className="page-inner">
        <header className="page-header">
        <div>
          <h1>Settings</h1>
          <p className="subtitle">Game setup, display units, appearance, and app preferences.</p>
        </div>
      </header>

      <div className="settings-tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={tab === t.id ? "" : "secondary"}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div className="settings-panel">
        {tab === "setup" && <GameSetupPanel />}

        {tab === "units" && (
          <div className="settings-section">
            <h2>Speed</h2>
            <p className="muted">How speed is shown on charts.</p>
            <div className="radio-row">
              <label>
                <input
                  type="radio"
                  name="speedUnit"
                  checked={prefs.speedUnit === "kmh"}
                  onChange={() => setPrefs({ speedUnit: "kmh" as SpeedUnit })}
                />
                km/h
              </label>
              <label>
                <input
                  type="radio"
                  name="speedUnit"
                  checked={prefs.speedUnit === "mph"}
                  onChange={() => setPrefs({ speedUnit: "mph" as SpeedUnit })}
                />
                mph
              </label>
            </div>

            <h2>Time delta</h2>
            <p className="muted">Format for lap time difference charts.</p>
            <div className="radio-row">
              <label>
                <input
                  type="radio"
                  name="deltaUnit"
                  checked={prefs.deltaUnit === "s"}
                  onChange={() => setPrefs({ deltaUnit: "s" as DeltaUnit })}
                />
                Seconds (e.g. +0.123 s)
              </label>
              <label>
                <input
                  type="radio"
                  name="deltaUnit"
                  checked={prefs.deltaUnit === "ms"}
                  onChange={() => setPrefs({ deltaUnit: "ms" as DeltaUnit })}
                />
                Milliseconds (e.g. +123 ms)
              </label>
            </div>

            <h2>Inputs</h2>
            <p className="muted">
              Throttle and brake are shown as %. ACC steering is degrees on a
              ±100° scale (100 = full lock), labeled L/R. Other games show
              steering as L/R with % magnitude.
            </p>

            <h2>Fuel</h2>
            <p className="muted">Review session fuel-used column and fuel charts.</p>
            <div className="radio-row">
              <label>
                <input
                  type="radio"
                  name="fuelUnit"
                  checked={prefs.fuelUnit === "l"}
                  onChange={() => setPrefs({ fuelUnit: "l" as FuelUnit })}
                />
                Liters (L)
              </label>
              <label>
                <input
                  type="radio"
                  name="fuelUnit"
                  checked={prefs.fuelUnit === "us_gal"}
                  onChange={() => setPrefs({ fuelUnit: "us_gal" as FuelUnit })}
                />
                US gallons
              </label>
            </div>

            <h2>Temperature</h2>
            <p className="muted">Tyre core temperature charts and tooltips.</p>
            <div className="radio-row">
              <label>
                <input
                  type="radio"
                  name="tempUnit"
                  checked={prefs.tempUnit === "c"}
                  onChange={() => setPrefs({ tempUnit: "c" as TempUnit })}
                />
                Celsius (°C)
              </label>
              <label>
                <input
                  type="radio"
                  name="tempUnit"
                  checked={prefs.tempUnit === "f"}
                  onChange={() => setPrefs({ tempUnit: "f" as TempUnit })}
                />
                Fahrenheit (°F)
              </label>
            </div>

            <h2>Pressure</h2>
            <p className="muted">Tyre pressure charts and tooltips.</p>
            <div className="radio-row">
              <label>
                <input
                  type="radio"
                  name="pressureUnit"
                  checked={prefs.pressureUnit === "psi"}
                  onChange={() => setPrefs({ pressureUnit: "psi" as PressureUnit })}
                />
                PSI
              </label>
              <label>
                <input
                  type="radio"
                  name="pressureUnit"
                  checked={prefs.pressureUnit === "bar"}
                  onChange={() => setPrefs({ pressureUnit: "bar" as PressureUnit })}
                />
                bar
              </label>
            </div>
          </div>
        )}

        {tab === "appearance" && (
          <div className="settings-section">
            <h2>Background</h2>
            <p className="muted">The picked color is the page background. Panels, controls, and text follow the palette recipe.</p>
            <div className="preset-swatches">
              {Object.entries(BACKGROUND_PRESETS).map(([id, color]) => (
                <button
                  key={id}
                  type="button"
                  className={`preset-swatch ${prefs.appearance.backgroundPreset === id && !prefs.appearance.backgroundCustom ? "selected" : ""}`}
                  style={{ background: color }}
                  title={id}
                  onClick={() =>
                    setPrefs({
                      appearance: {
                        backgroundPreset: id,
                        backgroundCustom: "",
                      },
                    })
                  }
                />
              ))}
            </div>
            <div className="form-row appearance-custom-row">
              <label className="form-field">
                <span className="form-label">Custom color</span>
                <input
                  type="color"
                  value={
                    prefs.appearance.backgroundCustom ||
                    BACKGROUND_PRESETS[prefs.appearance.backgroundPreset]
                  }
                  onChange={(e) =>
                    setPrefs({
                      appearance: { backgroundCustom: e.target.value },
                    })
                  }
                />
              </label>
            </div>

            <h2>Lap series colors</h2>
            <p className="muted">Colors for compared laps in analysis charts.</p>
            <div className="color-editor-grid">
              {prefs.appearance.lapColors.map((color, i) => (
                <label key={i} className="color-editor-item">
                  <span>Lap {i + 1}</span>
                  <input
                    type="color"
                    value={color}
                    onChange={(e) => updateLapColor(i, e.target.value)}
                  />
                </label>
              ))}
            </div>

            <button
              type="button"
              className="secondary"
              onClick={() => resetAppearance()}
            >
              Reset appearance to defaults
            </button>
          </div>
        )}

        {tab === "general" && (
          <div className="settings-section">
            <h2>Data storage</h2>
            <p className="muted">Sessions and lap files are stored locally.</p>
            <p><code className="path-block">{dataDir || "—"}</code></p>
            {dataDir && (
              <button
                type="button"
                className="secondary"
                onClick={() => openPath(dataDir)}
              >
                Open data folder
              </button>
            )}

            <h2>About</h2>
            <p className="muted">SimTelemetry v0.1.0</p>
          </div>
        )}
      </div>
      </div>
    </div>
  );
}