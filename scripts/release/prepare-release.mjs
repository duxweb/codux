#!/usr/bin/env node
/* global console, process */

import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const rawRef = process.argv[2] || process.env.GITHUB_REF_NAME || "";
const rawChannel = process.argv[3] || process.env.RELEASE_CHANNEL || "";
const version = normalizeVersion(rawRef);
const bundleVersion = process.env.RELEASE_BUNDLE_VERSION || normalizedBundleVersion(version);
const channel =
  rawChannel === "stable" || rawChannel === "beta" ? rawChannel : version.includes("-") ? "beta" : "stable";

updateJson("package.json", (data) => {
  data.version = version;
  return data;
});
updateTauriVersion("src-tauri/tauri.conf.json", version);
updateBundleVersion("src-tauri/tauri.conf.json", bundleVersion);
updateCargoVersion("src-tauri/Cargo.toml", version);
updateCargoLockVersion("src-tauri/Cargo.lock", version);
updateSettingsVersion("src/settings.ts", version);

const notesPath =
  process.env.RELEASE_NOTES_PATH ||
  (process.env.GITHUB_OUTPUT ? "" : path.join(root, "dist", `release-notes-${version}.md`));
const notes = extractChangelogSection("CHANGELOG.md", version);
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
    .replace(/^tauri-v/, "")
    .replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(trimmed)) {
    throw new Error(`Invalid release version "${value}". Expected a SemVer tag such as v1.0.0-beta.1.`);
  }
  return trimmed;
}

function normalizedBundleVersion(value) {
  if (!process.env.RELEASE_WINDOWS_BUNDLE_VERSION) {
    return value;
  }
  const match = value.match(/^(\d+)\.(\d+)\.(\d+)(?:-[A-Za-z]+\.?(\d+))?/);
  if (!match) {
    return value;
  }
  return `${match[1]}.${match[2]}.${match[3]}.${match[4] || "0"}`;
}

function updateJson(relativePath, updater) {
  const filePath = path.join(root, relativePath);
  const data = JSON.parse(fs.readFileSync(filePath, "utf8"));
  fs.writeFileSync(filePath, `${JSON.stringify(updater(data), null, 2)}\n`, "utf8");
}

function updateCargoVersion(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  const content = fs.readFileSync(filePath, "utf8");
  fs.writeFileSync(filePath, content.replace(/^version = ".*"$/m, `version = "${nextVersion}"`), "utf8");
}

function updateTauriVersion(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  const content = fs.readFileSync(filePath, "utf8");
  fs.writeFileSync(filePath, content.replace(/^ {2}"version": ".*",$/m, `  "version": "${nextVersion}",`), "utf8");
}

function updateBundleVersion(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  const content = fs.readFileSync(filePath, "utf8");
  fs.writeFileSync(
    filePath,
    content.replace(/^ {6}"bundleVersion": ".*",$/m, `      "bundleVersion": "${nextVersion}",`),
    "utf8",
  );
}

function updateCargoLockVersion(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  const content = fs.readFileSync(filePath, "utf8");
  fs.writeFileSync(filePath, content.replace(/(name = "codux-tauri"\nversion = )".*"/, `$1"${nextVersion}"`), "utf8");
}

function updateSettingsVersion(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  const content = fs.readFileSync(filePath, "utf8");
  fs.writeFileSync(
    filePath,
    content.replace(/const APP_VERSION = ".*";/, `const APP_VERSION = "${nextVersion}";`),
    "utf8",
  );
}

function extractChangelogSection(relativePath, nextVersion) {
  const filePath = path.join(root, relativePath);
  const lines = fs.readFileSync(filePath, "utf8").split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith(`## [${nextVersion}]`));
  if (start === -1) {
    return `Codux ${nextVersion}`;
  }
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## ["));
  return lines
    .slice(start, end === -1 ? undefined : end)
    .join("\n")
    .trim();
}
