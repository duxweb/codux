#!/usr/bin/env node
/* global console, process */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = process.cwd();
const buildId = process.env.RELEASE_BUILD_ID || `${targetPlatformLabel()}-${targetArchLabel()}`;
const target = process.env.CARGO_BUILD_TARGET || "";
const profile = process.env.CARGO_PROFILE || "release";
const stageRoot = process.env.RELEASE_STAGE_DIR || "release-artifacts";
const platform = targetPlatformLabel();
const arch = targetArchLabel();
const extension = platform === "windows" ? ".exe" : "";
const outputDir = path.join(root, stageRoot, buildId);
const assetName = `codux-${platform}-${arch}${extension}`;

fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(outputDir, { recursive: true });

const binaryPath = releaseBinaryPath(extension);
const assetPath = path.join(outputDir, assetName);
fs.copyFileSync(binaryPath, assetPath);
if (platform !== "windows") {
  fs.chmodSync(assetPath, 0o755);
}

console.log(`Packaged ${assetName}`);

function releaseBinaryPath(binaryExtension) {
  const segments = [root, "target"];
  if (target) segments.push(target);
  segments.push(profile, `codux-agent${binaryExtension}`);
  const binaryPath = path.join(...segments);
  if (!fs.existsSync(binaryPath)) {
    throw new Error(`Built agent binary not found: ${binaryPath}`);
  }
  return binaryPath;
}

function targetPlatformLabel() {
  if (target.includes("apple-darwin")) return "macos";
  if (target.includes("linux")) return "linux";
  if (target.includes("windows")) return "windows";
  if (process.platform === "darwin") return "macos";
  if (process.platform === "linux") return "linux";
  if (process.platform === "win32") return "windows";
  throw new Error(`Unsupported agent release platform: ${process.platform || os.platform()}`);
}

function targetArchLabel() {
  if (target.includes("aarch64")) return "aarch64";
  if (target.includes("x86_64")) return "x86_64";
  if (process.arch === "arm64") return "aarch64";
  if (process.arch === "x64") return "x86_64";
  throw new Error(`Unsupported agent release architecture: ${process.arch || os.arch()}`);
}
