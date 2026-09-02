// Derive the build version from git, so every installer is named for what it
// was built from instead of the placeholder in tauri.conf.json.
//
//   on tag 0.1.2, clean tree  ->  0.1.2         (a release)
//   17 commits past 0.1.2     ->  0.1.3-dev.17  (the next patch, in progress)
//   ... with local edits      ->  0.1.3-dev.17.dirty
//
// MSI product versions must be numeric (major/minor <= 255, the rest <= 65535),
// so the same build also gets a `wixVersion` — 0.1.3.17 for the example above.
// NSIS takes the full string and derives its own numeric VIProductVersion.
//
// Run it directly (`node scripts/app-version.mjs`) to print what a build here
// would be called.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const TAURI_CONFIG = path.join(ROOT, "src-tauri", "tauri.conf.json");

/// Tags that name a release. `beta` and friends are ignored.
const RELEASE_TAG_GLOBS = ["[0-9]*.[0-9]*.[0-9]*", "v[0-9]*.[0-9]*.[0-9]*"];
const RELEASE_TAG = /^v?(\d+)\.(\d+)\.(\d+)$/;
/// `git describe --long` output: <tag>-<commits since it>-g<short sha>.
const DESCRIBE = /^(.+)-(\d+)-g([0-9a-f]+)$/;

const MSI_FIELD_MAX = 65_535;

function git(...args) {
  return execFileSync("git", args, {
    cwd: ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function tryGit(...args) {
  try {
    return git(...args);
  } catch {
    return null;
  }
}

export function readConfigVersion() {
  const config = JSON.parse(readFileSync(TAURI_CONFIG, "utf8"));
  return config.version ?? "0.0.0";
}

/// The version this working tree would build as.
export function resolveVersion() {
  const configVersion = readConfigVersion();
  const described = tryGit(
    "describe",
    "--tags",
    "--long",
    ...RELEASE_TAG_GLOBS.flatMap((glob) => ["--match", glob]),
  );
  const head = tryGit("rev-parse", "--short", "HEAD");
  if (head === null) {
    // No git to ask (a source tarball, say) — leave the config version alone.
    return { version: configVersion, wixVersion: null, source: "tauri.conf.json" };
  }

  const dirty = tryGit("status", "--porcelain") !== "";
  const parts = described === null ? null : DESCRIBE.exec(described);
  const tag = parts === null ? null : RELEASE_TAG.exec(parts[1]);

  // Without a release tag to count from, fall back to the config version and
  // the whole history as the distance.
  const [major, minor, patch] = tag
    ? tag.slice(1, 4).map(Number)
    : configVersion.split(/[.\-+]/).slice(0, 3).map(Number);
  const distance = parts ? Number(parts[2]) : Number(tryGit("rev-list", "--count", "HEAD") ?? 0);
  const source = tag ? `${parts[1]}+${distance} g${parts[3]}` : `${configVersion} (no release tag) g${head}`;

  if (tag && distance === 0 && !dirty) {
    return { version: parts[1].replace(/^v/, ""), wixVersion: null, source, release: true };
  }

  // Anything else is work in progress towards the next patch.
  const next = `${major}.${minor}.${patch + 1}`;
  const build = Math.min(distance, MSI_FIELD_MAX);
  return {
    version: `${next}-dev.${distance}${dirty ? ".dirty" : ""}`,
    wixVersion: `${next}.${build}`,
    source,
    release: false,
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const resolved = resolveVersion();
  console.log(JSON.stringify(resolved, null, 2));
}
