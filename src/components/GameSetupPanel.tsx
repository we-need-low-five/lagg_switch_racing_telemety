import { useCallback, useEffect, useState } from "react";
import { checkGameSetup } from "../api";
import type { GameId, GameSetupStatus } from "../types";
import { gameLabel } from "../types";

const GAMES: Array<{ id: GameId; title: string; steps: string[] }> = [
  {
    id: "acc",
    title: "Assetto Corsa Competizione",
    steps: [
      "Launch ACC and enter any on-track session.",
      "Shared memory is enabled by default — no plugin required.",
      "Drive a timed lap; SimTelemetry will auto-detect telemetry.",
    ],
  },
  {
    id: "ac",
    title: "Assetto Corsa",
    steps: [
      "Launch Assetto Corsa and enter a practice or race session.",
      "Shared memory is enabled by default.",
      "Drive on track to activate telemetry capture.",
    ],
  },
  {
    id: "lmu",
    title: "Le Mans Ultimate",
    steps: [
      "Ensure LMU 1.2+ with official shared memory enabled.",
      "Enter a session and drive on track.",
      "If telemetry fails, verify Support/SharedMemoryInterface is active.",
    ],
  },
  {
    id: "f1_25",
    title: "F1 25",
    steps: [
      "In-game: Settings → Telemetry Settings.",
      "Set UDP IP Address to 127.0.0.1 and port 20777.",
      "Set UDP Format to 2025 and Send Rate to 20 Hz.",
      "Start a timed lap — incomplete laps are ignored by the game.",
    ],
  },
];

export function GameSetupPanel() {
  const [statuses, setStatuses] = useState<Record<GameId, GameSetupStatus>>(
    {} as Record<GameId, GameSetupStatus>,
  );

  const refresh = useCallback(async () => {
    const checks = await Promise.all(GAMES.map((g) => checkGameSetup(g.id)));
    const map = {} as Record<GameId, GameSetupStatus>;
    for (const status of checks) map[status.game] = status;
    setStatuses(map);
  }, []);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 4000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  return (
    <div>
      <div className="setup-panel-toolbar panel-toolbar">
        <p className="muted setup-panel-intro">
          Verify telemetry for each game. SimTelemetry runs in the system tray while you drive.
        </p>
        <button type="button" onClick={refresh}>Re-check</button>
      </div>
      <div className="setup-grid">
        {GAMES.map((game) => {
          const status = statuses[game.id];
          const ok = status?.telemetry_active;
          const partial = status?.process_detected && !status?.telemetry_active;
          return (
            <article
              key={game.id}
              className={`setup-card ${ok ? "ok" : partial ? "partial" : ""}`}
            >
              <div className="setup-card-header">
                <h2>{game.title}</h2>
                <span className="badge">{gameLabel(game.id)}</span>
              </div>
              <ol>
                {game.steps.map((step) => (
                  <li key={step}>{step}</li>
                ))}
              </ol>
              <div className="setup-status">
                <span className={`status-dot ${ok ? "ok" : partial ? "partial" : ""}`} />
                <span>{status?.message ?? "Checking…"}</span>
              </div>
            </article>
          );
        })}
      </div>
    </div>
  );
}
