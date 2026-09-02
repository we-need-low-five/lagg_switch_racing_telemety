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

## Versioning

`npm run tauri:build` derives the version from git (`scripts/app-version.mjs`) and
hands it to the Tauri CLI with `--config`, so installers are named for the commit
they came from and nothing in the repo is rewritten:

| working tree | version | artifact |
|---|---|---|
| on tag `0.1.2`, clean | `0.1.2` | `SimTelemetry_0.1.2_x64_en-US.msi` |
| 17 commits past `0.1.2` | `0.1.3-dev.17` | `SimTelemetry_0.1.3-dev.17_x64-setup.exe` |
| …with local edits | `0.1.3-dev.17.dirty` | same, `.dirty` in the name |

`npm run version:show` prints what a build here would be called, without building.

Cutting a release is therefore: tag `x.y.z` on the commit, then build. Only tags
shaped like a version are counted (`beta` and the like are ignored), and the
version in `tauri.conf.json` is just the floor used when no release tag is
reachable — it never has to be edited.

MSI product versions must be numeric, so a dev build also passes
`bundle.windows.wix.version` (`0.1.3.17` for the example above). Windows ignores
that fourth field when deciding whether one MSI upgrades another, so two dev
builds of the same `0.1.3` won't upgrade over each other — use the NSIS
`-setup.exe` for those, or uninstall first. Tagged releases have distinct
`major.minor.patch` and upgrade normally.

## First-run Checklist

1. Install SimTelemetry.
2. Launch the app — it stays in the system tray.
3. Open **Setup Wizard** and verify your game(s).
4. Drive a session; open **Sessions** afterward to analyze laps.

Game setup guides are bundled under `docs/game-setup/` in the repository and referenced from the in-app wizard.
