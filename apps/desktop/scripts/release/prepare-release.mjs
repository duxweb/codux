#!/usr/bin/env node
/* global console, process */

import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const dryRun = process.argv.includes("--dry-run");
const rawRef = process.argv[2] || process.env.GITHUB_REF_NAME || "";
const rawChannel = process.argv[3] || process.env.RELEASE_CHANNEL || "";
const version = normalizeVersion(rawRef);
const channel =
  rawChannel === "stable" || rawChannel === "beta" ? rawChannel : version.includes("-") ? "beta" : "stable";

if (!dryRun) {
  updateCargoVersion("apps/desktop/Cargo.toml", version);
  updateCargoVersion("apps/desktop/runtime/Cargo.toml", version);
  updateCargoLockPackageVersion("Cargo.lock", "codux", version);
  updateCargoLockPackageVersion("Cargo.lock", "codux-runtime", version);
}

const notesPath =
  process.env.RELEASE_NOTES_PATH ||
  (process.env.GITHUB_OUTPUT ? "" : path.join(root, "dist", `release-notes-${version}.md`));
const notes = buildReleaseNotes(version);
if (notesPath) {
  fs.mkdirSync(path.dirname(notesPath), { recursive: true });
  fs.writeFileSync(notesPath, notes, "utf8");
}

if (process.env.GITHUB_OUTPUT) {
  fs.appendFileSync(
    process.env.GITHUB_OUTPUT,
    [
      `version=${version}`,
      `channel=${channel}`,
      `notes<<__CODUX_RELEASE_NOTES__`,
      notes,
      `__CODUX_RELEASE_NOTES__`,
    ].join("\n") + "\n",
  );
} else {
  console.log(`version=${version}`);
  console.log(`channel=${channel}`);
  console.log(notes);
}

function normalizeVersion(value) {
  const trimmed = value
    .trim()
    .replace(/^refs\/tags\//, "")
    .replace(/^gpui-v/, "")
    .replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(trimmed)) {
    throw new Error(`Invalid release version "${value}". Expected a SemVer tag such as v1.0.0-beta.1.`);
  }
  return trimmed;
}

function updateCargoVersion(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  const content = fs.readFileSync(filePath, "utf8");
  fs.writeFileSync(filePath, content.replace(/^version = ".*"$/m, `version = "${nextVersion}"`), "utf8");
}

function updateCargoLockPackageVersion(relativePath, packageName, nextVersion) {
  const filePath = path.join(root, relativePath);
  if (!fs.existsSync(filePath)) return;
  const content = fs.readFileSync(filePath, "utf8");
  const pattern = new RegExp(`(name = "${escapeRegExp(packageName)}"\\nversion = )".*"`);
  fs.writeFileSync(filePath, content.replace(pattern, `$1"${nextVersion}"`), "utf8");
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function extractChangelogSection(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  if (!fs.existsSync(filePath)) {
    return "";
  }
  const lines = fs.readFileSync(filePath, "utf8").split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith(`## [${nextVersion}]`));
  if (start === -1) {
    return "";
  }
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## ["));
  return lines
    .slice(start, end === -1 ? undefined : end)
    .join("\n")
    .trim();
}

function buildReleaseNotes(nextVersion) {
  const english = extractChangelogSection("CHANGELOG.md", nextVersion);
  const chinese = extractChangelogSection("CHANGELOG.zh-CN.md", nextVersion);
  if (chinese && english) {
    return [`## 中文`, chinese, `---`, `## English`, english].join("\n\n");
  }
  return english || chinese || `Codux ${nextVersion}`;
}
