import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const APP_TITLE = "SimTelemetry";

/** `getCurrentWindow` throws outside the Tauri shell (plain `npm run dev`). */
function appWindow() {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = appWindow();
    if (!win) return;

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const sync = () => {
      win
        .isMaximized()
        .then((value) => {
          if (!cancelled) setMaximized(value);
        })
        .catch(() => {});
    };

    sync();
    win
      .onResized(sync)
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-rail" data-tauri-drag-region />
      <div className="titlebar-main" data-tauri-drag-region>
        <span className="titlebar-title" data-tauri-drag-region>
          {APP_TITLE}
        </span>
        <div className="titlebar-controls">
          <button
            type="button"
            className="titlebar-button"
            aria-label="Minimize"
            title="Minimize"
            onClick={() => appWindow()?.minimize()}
          >
            <svg viewBox="0 0 10 10" aria-hidden focusable="false">
              <path d="M0 5h10" />
            </svg>
          </button>
          <button
            type="button"
            className="titlebar-button"
            aria-label={maximized ? "Restore" : "Maximize"}
            title={maximized ? "Restore" : "Maximize"}
            onClick={() => appWindow()?.toggleMaximize()}
          >
            {maximized ? (
              <svg viewBox="0 0 10 10" aria-hidden focusable="false">
                <path d="M0.5 2.5h7v7h-7z" />
                <path d="M2.5 2.5v-2h7v7h-2" />
              </svg>
            ) : (
              <svg viewBox="0 0 10 10" aria-hidden focusable="false">
                <path d="M0.5 0.5h9v9h-9z" />
              </svg>
            )}
          </button>
          <button
            type="button"
            className="titlebar-button close"
            aria-label="Close"
            title="Close"
            onClick={() => appWindow()?.close()}
          >
            <svg viewBox="0 0 10 10" aria-hidden focusable="false">
              <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
            </svg>
          </button>
        </div>
      </div>
    </header>
  );
}
