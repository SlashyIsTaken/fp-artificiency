#!/usr/bin/env node
// One-command release: guard, bump, commit, tag, push. The tag push triggers
// the Release workflow (.github/workflows/release.yml). Run: `npm run release <version>`.
import { execFileSync } from "node:child_process";
import { bump, assertValidVersion } from "./bump-version.mjs";

const BRANCH = "main"; // releases are cut from main only
const REMOTE = "origin";

function fail(msg) {
  console.error(`✗ ${msg}`);
  process.exit(1);
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
const gitInherit = (...args) => execFileSync("git", args, { stdio: "inherit" });

function tagExistsLocal(tag) {
  try {
    execFileSync("git", ["rev-parse", "-q", "--verify", `refs/tags/${tag}`], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

const version = process.argv[2];
if (!version) fail("usage: npm run release <version>   (e.g. npm run release 0.1.2)");
try {
  assertValidVersion(version);
} catch (e) {
  fail(e.message);
}
const tag = `v${version}`;

// ── Guards (fail before touching anything) ──────────────────────────────────
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
if (branch !== BRANCH) fail(`on branch "${branch}"; releases must be cut from "${BRANCH}".`);

if (git("status", "--porcelain")) {
  fail("working tree has uncommitted changes; commit or stash them so the bump commit is clean.");
}

if (tagExistsLocal(tag)) fail(`tag ${tag} already exists locally.`);
try {
  if (git("ls-remote", "--tags", REMOTE, tag)) fail(`tag ${tag} already exists on ${REMOTE}.`);
} catch (e) {
  fail(`could not reach ${REMOTE} to check for ${tag}: ${e.message}`);
}

// ── Verify, then bump → commit → tag → push ─────────────────────────────────
console.log("→ running checks (npm run check)…");
try {
  execFileSync("npm", ["run", "check"], { stdio: "inherit" });
} catch {
  fail("checks failed; not releasing.");
}

console.log(`→ bumping to ${version}`);
try {
  bump(version);
} catch (e) {
  fail(e.message);
}

// Cargo.toml's bump leaves Cargo.lock's workspace-member versions stale; resync
// them so the lock lands in the same commit. --workspace touches only the local
// crates, never registry deps.
console.log("→ syncing Cargo.lock (cargo update --workspace)…");
try {
  execFileSync("cargo", ["update", "--workspace"], { stdio: "inherit" });
} catch {
  fail("cargo update --workspace failed; not releasing.");
}

try {
  gitInherit("commit", "-am", `chore: bump version to ${tag}`);
  gitInherit("tag", tag);
} catch (e) {
  fail(`git commit/tag failed: ${e.message}`);
}

try {
  gitInherit("push", REMOTE, BRANCH, tag);
} catch (e) {
  fail(`push failed: ${e.message}\n  commit and ${tag} exist locally; retry with: git push ${REMOTE} ${BRANCH} ${tag}`);
}

console.log(`\n✓ released ${tag}. The Release workflow is now building.`);
