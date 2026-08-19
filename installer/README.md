# SimTelemetry Windows Installer Notes

Production builds use Tauri's bundler with MSI and NSIS targets configured in `src-tauri/tauri.conf.json`.

## Build

```powershell
npm install
npm run tauri:build
```

Artifacts:

- `src-tauri/target/release/bundle/msi/`
- `src-tauri/target/release/bundle/nsis/`

## First-run Checklist

1. Install SimTelemetry.
2. Launch the app — it stays in the system tray.
3. Open **Setup Wizard** and verify your game(s).
4. Drive a session; open **Sessions** afterward to analyze laps.

Game setup guides are bundled under `docs/game-setup/` in the repository and referenced from the in-app wizard.
