#!/usr/bin/env node
/* global console, process */

import fs from "node:fs";
import path from "node:path";

const sourceDir = requiredEnv("BUNDLE_SOURCE_DIR");
const buildId = requiredEnv("RELEASE_BUILD_ID");
const outputRoot = process.env.RELEASE_STAGE_DIR || "release-artifacts";
const outputDir = path.join(outputRoot, buildId);
const allowed = [
  ".app.tar.gz.sig",
  ".app.tar.gz",
  ".msi.zip.sig",
  ".nsis.zip.sig",
  ".msi.sig",
  ".exe.sig",
  ".dmg.sig",
  ".dmg",
  ".msi",
  ".exe",
  ".zip",
  ".sig",
];

fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(outputDir, { recursive: true });

const files = walk(sourceDir).filter(shouldStage);
if (!files.length) {
  throw new Error(`No bundle artifacts found in ${sourceDir}`);
}

for (const file of files) {
  const name = normalizeAssetName(path.relative(sourceDir, file));
  fs.copyFileSync(file, path.join(outputDir, name));
  console.log(`staged ${name}`);
}

function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function walk(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : [file];
  });
}

function shouldStage(file) {
  if (file.endsWith(".blockmap") || file.endsWith("latest.json")) return false;
  return allowed.some((ext) => file.endsWith(ext));
}

function normalizeAssetName(relativePath) {
  return `${buildId}-${relativePath
    .split(path.sep)
    .join("-")
    .replace(/[ ()[\]{}]/g, ".")
    .replace(/\.\./g, ".")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")}`;
}
