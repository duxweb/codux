#!/usr/bin/env node
/* global console, process */

import fs from "node:fs";
import path from "node:path";

const sourceDir = requiredEnv("BUNDLE_SOURCE_DIR");
const buildId = requiredEnv("RELEASE_BUILD_ID");
const artifactBaseName = process.env.RELEASE_ARTIFACT_BASENAME?.trim() || "";
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
  const relativePath = path.relative(sourceDir, file);
  const name = artifactBaseName ? normalizeNamedAsset(relativePath) : `${buildId}-${sanitizeName(relativePath)}`;
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

function normalizeNamedAsset(relativePath) {
  const ext = artifactExt(relativePath);
  if (ext === ".dmg") return `${artifactBaseName}.dmg`;
  if (ext === ".dmg.sig") return `${artifactBaseName}.dmg.sig`;
  if (ext === ".app.tar.gz") return `${artifactBaseName}-updater.app.tar.gz`;
  if (ext === ".app.tar.gz.sig") return `${artifactBaseName}-updater.app.tar.gz.sig`;
  return `${artifactBaseName}-${sanitizeName(relativePath)}`;
}

function artifactExt(file) {
  const name = path.basename(file);
  const known = [
    ".app.tar.gz.sig",
    ".app.tar.gz",
    ".tar.gz.sig",
    ".tar.gz",
    ".msi.zip.sig",
    ".nsis.zip.sig",
    ".dmg.sig",
    ".msi.sig",
    ".exe.sig",
  ];
  return known.find((ext) => name.endsWith(ext)) || path.extname(name);
}

function sanitizeName(relativePath) {
  return relativePath
    .split(path.sep)
    .join("-")
    .replace(/[ ()[\]{}]/g, ".")
    .replace(/\.\./g, ".")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "");
}
