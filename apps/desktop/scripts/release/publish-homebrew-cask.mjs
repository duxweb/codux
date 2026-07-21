#!/usr/bin/env node
/* global console, process */

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const version = requiredEnv("RELEASE_VERSION").replace(/^v/, "");
const token = requiredEnv("HOMEBREW_TAP_TOKEN");
const artifactsDir = process.env.RELEASE_ARTIFACTS_DIR || path.join(root, "release-artifacts");
const tapRepo = process.env.HOMEBREW_TAP_REPO || "duxweb/homebrew-tap";
const armDmgPath = findFormalDmg(artifactsDir, "aarch64");
const intelDmgPath = findFormalDmg(artifactsDir, "x86_64");
const armSha256 = sha256File(armDmgPath);
const intelSha256 = sha256File(intelDmgPath);
const tapDir = fs.mkdtempSync(path.join(os.tmpdir(), "codux-homebrew-tap-"));

try {
  const tapRepoUrl = `https://x-access-token:${token}@github.com/${tapRepo}.git`;
  run("git", ["clone", tapRepoUrl, tapDir], { mask: token });
  const caskPath = path.join(tapDir, "Casks", "codux.rb");
  run("node", [
    "apps/desktop/scripts/release/render-homebrew-cask.mjs",
    version,
    armSha256,
    intelSha256,
    caskPath,
  ]);

  if (git(["diff", "--quiet", "--", "Casks/codux.rb"], { cwd: tapDir }).status === 0) {
    console.log("[tap] cask already up to date");
    writeSummary("- Skipped: cask is already up to date.");
    process.exit(0);
  }

  run("git", ["config", "user.name", "github-actions[bot]"], { cwd: tapDir });
  run("git", ["config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], { cwd: tapDir });
  run("git", ["add", "Casks/codux.rb"], { cwd: tapDir });
  run("git", ["commit", "-m", `Update codux cask to v${version}`], { cwd: tapDir });
  run("git", ["push", "origin", "HEAD:main"], { cwd: tapDir, mask: token });
  writeSummary(`- Published: \`${tapRepo}\` updated to \`v${version}\`.`);
} finally {
  fs.rmSync(tapDir, { recursive: true, force: true });
}

function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function findFormalDmg(dir, arch) {
  const files = walk(dir).filter((file) => file.endsWith(".dmg"));
  const formal = files.find((file) => path.basename(file) === `codux-macos-${arch}.dmg`);
  if (formal) return formal;
  throw new Error(`Unable to find formal macOS ${arch} DMG in ${dir}`);
}

function walk(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : [file];
  });
}

function sha256File(file) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(file));
  return hash.digest("hex");
}

function run(command, args, options = {}) {
  const result = gitOrSpawn(command, args, options);
  if (result.status !== 0) {
    const rendered = `${command} ${args.join(" ")}`.replaceAll(options.mask || "\0", "***");
    throw new Error(`${rendered} failed with exit code ${result.status}`);
  }
}

function git(args, options = {}) {
  return gitOrSpawn("git", args, options);
}

function gitOrSpawn(command, args, options = {}) {
  return spawnSync(command, args, {
    cwd: options.cwd || root,
    stdio: "inherit",
    env: process.env,
  });
}

function writeSummary(line) {
  const summary = process.env.GITHUB_STEP_SUMMARY;
  if (!summary) return;
  fs.appendFileSync(summary, `### Homebrew tap\n\n${line}\n`, "utf8");
}
