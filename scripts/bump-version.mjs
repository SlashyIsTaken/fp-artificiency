#!/usr/bin/env node
// Single source of truth for the app version: rewrites all three places that
// carry it so a release bump is one command. Run: `npm run bump <version>`.
//
// The two Rust crates inherit from [workspace.package], so only the workspace
// Cargo.toml is touched, not the per-crate manifests.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const version = process.argv[2];
if (!version) {
  console.error("usage: npm run bump <version>   (e.g. npm run bump 0.1.1)");
  process.exit(1);
}
// Bundlers reject non-semver, so fail here rather than deep in a release build.
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`invalid semver version: "${version}"`);
  process.exit(1);
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

for (const { file, re } of edits) {
  const path = join(root, file);
  const before = readFileSync(path, "utf8");
  if (!re.test(before)) {
    console.error(`could not find version field in ${file}; aborting.`);
    process.exit(1);
  }
  const after = before.replace(re, `$1${version}$2`);
  writeFileSync(path, after);
  console.log(`  ${file} -> ${version}`);
}

console.log(`\nBumped to ${version}. Next: commit, then tag (git tag v${version}).`);
