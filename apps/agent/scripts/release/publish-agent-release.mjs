#!/usr/bin/env node
/* global console, process */

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const dryRun = process.argv.includes("--dry-run");
const version = requiredEnv("RELEASE_VERSION");
const tagName = process.env.RELEASE_TAG || `v${version}`;
const repo = process.env.GITHUB_REPOSITORY || "duxweb/codux";
const artifactsDir = process.env.RELEASE_ARTIFACTS_DIR || path.join(root, "release-artifacts");

const assets = collectAgentAssets(artifactsDir);
if (!assets.length) {
  throw new Error(`No agent release assets found in ${artifactsDir}`);
}

if (!dryRun) {
  assertReleaseExists();
  for (const asset of assets) {
    run("gh", ["release", "upload", tagName, "--repo", repo, "--clobber", `${asset.path}#${asset.name}`]);
  }
}

console.log(`${dryRun ? "Prepared" : "Published"} ${assets.length} agent assets to ${repo}@${tagName}`);

function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function collectAgentAssets(dir) {
  return walk(dir)
    .map((file) => ({
      path: file,
      name: path.basename(file),
    }))
    .filter((asset) => isAgentAsset(asset.name))
    .sort((left, right) => left.name.localeCompare(right.name));
}

function walk(dir) {
  if (!fs.existsSync(dir)) return [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  return entries.flatMap((entry) => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : [file];
  });
}

function isAgentAsset(name) {
  return (
    new RegExp(`^codux-agent-${escapeRegExp(version)}-(macos|linux|windows)-(aarch64|x86_64)(?:\\.exe)?$`).test(
      name,
    ) || /^codux-(macos|linux|windows)-(aarch64|x86_64)(?:\.exe)?$/.test(name)
  );
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function assertReleaseExists() {
  const result = spawnSync("gh", ["release", "view", tagName, "--repo", repo], {
    stdio: "ignore",
    env: process.env,
  });
  if (result.status !== 0) {
    throw new Error(`Release ${tagName} does not exist in ${repo}`);
  }
}

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit", env: process.env });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}
