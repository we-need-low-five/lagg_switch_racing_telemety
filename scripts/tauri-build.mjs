// `npm run tauri:build` — a release build whose installers are named for the
// commit they came from. The version is derived in scripts/app-version.mjs and
// handed to the Tauri CLI with `--config`, so nothing in the repo is rewritten
// and the working tree stays clean.
//
// Extra arguments are passed straight through:
//   npm run tauri:build -- --bundles nsis

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { run } from "@tauri-apps/cli";

import { resolveVersion } from "./app-version.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const TAURI_CONFIG = path.join(ROOT, "src-tauri", "tauri.conf.json");

const { version, wixVersion, source, release } = resolveVersion();

// Pass the whole config rather than a patch: `--config` merges over the file on
// disk, and sending the merged document keeps `bundle` intact however the CLI
// chooses to merge.
const config = JSON.parse(readFileSync(TAURI_CONFIG, "utf8"));
config.version = version;
if (wixVersion) {
  config.bundle ??= {};
  config.bundle.windows ??= {};
  config.bundle.windows.wix ??= {};
  config.bundle.windows.wix.version = wixVersion;
}

console.log(
  `Building ${config.productName} ${version}` +
    (wixVersion ? ` (msi ${wixVersion})` : "") +
    ` — ${release ? "release" : "dev build"} from ${source}`,
);

try {
  await run(["build", "--config", JSON.stringify(config), ...process.argv.slice(2)], "tauri");
} catch (error) {
  console.error(error?.message ?? error);
  process.exit(1);
}
