#!/usr/bin/env node
// Single source of truth for the app version: rewrites all three places that
// carry it so a release bump is one command. Run: `npm run bump <version>`.
//
// The two Rust crates inherit from [workspace.package], so only the workspace
// Cargo.toml is touched, not the per-crate manifests.
//
// Also the reusable core behind `npm run release`, which imports bump() and
// assertValidVersion() rather than shelling out.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Bundlers reject non-semver, so fail here rather than deep in a release build.
const SEMVER_RE = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

export function assertValidVersion(version) {
  if (!version || !SEMVER_RE.test(version)) {
    throw new Error(`invalid semver version: "${version ?? ""}"`);
  }
}

// Each edit is anchored so it hits the app version and nothing else (not the
// "@tauri-apps/*" ranges in package.json, not the `tauri = { version }` dep in
// Cargo.toml, not the `version.workspace = true` crate lines).
const edits = [
  {
    file: "package.json",
    // First "version" key is the top-level app version.
    re: /("version":\s*")[^"]*(")/,
  },
  {
    file: "src-tauri/tauri.conf.json",
    re: /("version":\s*")[^"]*(")/,
  },
  {
    file: "Cargo.toml",
    // Line-anchored `version = "..."` = the [workspace.package] one.
    re: /^(version = ")[^"]*(")/m,
  },
];

// Rewrite the app version across all three files. Throws if any field is missing.
// Cargo.lock is left to `cargo update --workspace` (see release.mjs), so bump
// stays a pure, offline file rewrite.
export function bump(version) {
  assertValidVersion(version);
  for (const { file, re } of edits) {
    const path = join(root, file);
    const before = readFileSync(path, "utf8");
    if (!re.test(before)) {
      throw new Error(`could not find version field in ${file}; aborting.`);
    }
    writeFileSync(path, before.replace(re, `$1${version}$2`));
    console.log(`  ${file} -> ${version}`);
  }
}

// CLI: `npm run bump <version>` stays a pure file-rewrite primitive.
if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const version = process.argv[2];
  if (!version) {
    console.error("usage: npm run bump <version>   (e.g. npm run bump 0.1.1)");
    process.exit(1);
  }
  try {
    bump(version);
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
  console.log(`\nBumped to ${version}. Next: commit, then tag (git tag v${version}).`);
}
