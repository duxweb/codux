#!/usr/bin/env node
/* global console, process */

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const dryRun = process.argv.includes("--dry-run");
const version = requiredEnv("RELEASE_VERSION");
const channel = requiredEnv("RELEASE_CHANNEL");
const tagName = process.env.RELEASE_TAG || `v${version}`;
const channelTag = `tauri-${channel}`;
const repo = process.env.GITHUB_REPOSITORY || "duxweb/codux";
const notesPath = process.env.RELEASE_NOTES_PATH || path.join(root, "dist", `release-notes-${version}.md`);
const artifactsDir = process.env.RELEASE_ARTIFACTS_DIR || path.join(root, "release-artifacts");
const notes = fs.existsSync(notesPath) ? fs.readFileSync(notesPath, "utf8") : `Codux ${version}`;

const assets = collectAssets(artifactsDir);
if (!assets.length) {
  throw new Error(`No release assets found in ${artifactsDir}`);
}

const latestJson = buildLatestJson(assets);
const latestPath = path.join(artifactsDir, "latest.json");
fs.writeFileSync(latestPath, `${JSON.stringify(latestJson, null, 2)}\n`, "utf8");

if (!dryRun) {
  run("gh", ["release", "delete", tagName, "--repo", repo, "--yes"], { allowFailure: true });
  run("gh", [
    "release",
    "create",
    tagName,
    "--repo",
    repo,
    "--title",
    `Codux ${version}`,
    "--notes-file",
    notesPath,
    channel === "beta" ? "--prerelease" : "--latest",
  ]);

  for (const asset of assets) {
    run("gh", ["release", "upload", tagName, "--repo", repo, "--clobber", `${asset.path}#${asset.name}`]);
  }

  run("gh", ["release", "delete", channelTag, "--repo", repo, "--yes", "--cleanup-tag"], { allowFailure: true });
  run("gh", [
    "release",
    "create",
    channelTag,
    "--repo",
    repo,
    "--target",
    tagName,
    "--title",
    `Codux ${channel}`,
    "--notes",
    `Codux ${channel} updater channel`,
    channel === "beta" ? "--prerelease" : "--latest",
  ]);
  run("gh", ["release", "upload", channelTag, "--repo", repo, "--clobber", `${latestPath}#latest.json`]);
}

console.log(
  `${dryRun ? "Prepared" : "Published"} ${assets.length} assets to ${repo}@${tagName} and updater metadata to ${channelTag}`,
);

function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function collectAssets(dir) {
  const files = walk(dir)
    .filter((file) => !file.endsWith(".blockmap"))
    .filter((file) => !file.endsWith("latest.json"));
  return files
    .map((file) => ({
      path: file,
      name: normalizeAssetName(path.relative(dir, file)),
      ext: artifactExt(file),
    }))
    .filter((asset) => shouldUpload(asset));
}

function walk(dir) {
  if (!fs.existsSync(dir)) return [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  return entries.flatMap((entry) => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : [file];
  });
}

function normalizeAssetName(relativePath) {
  return relativePath
    .split(path.sep)
    .join("-")
    .replace(/[ ()[\]{}]/g, ".")
    .replace(/\.\./g, ".")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "");
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
    ".msi.sig",
    ".exe.sig",
  ];
  return known.find((ext) => name.endsWith(ext)) || path.extname(name);
}

function shouldUpload(asset) {
  return [
    ".dmg",
    ".msi",
    ".exe",
    ".zip",
    ".app.tar.gz",
    ".sig",
    ".app.tar.gz.sig",
    ".msi.zip.sig",
    ".nsis.zip.sig",
    ".msi.sig",
    ".exe.sig",
  ].includes(asset.ext);
}

function buildLatestJson(assets) {
  const content = {
    version,
    notes,
    pub_date: new Date().toISOString(),
    platforms: {},
  };

  const byName = new Map(assets.map((asset) => [asset.name, asset]));
  const signatureAssets = assets
    .filter((asset) => asset.name.endsWith(".sig"))
    .sort((a, b) => signaturePriority(b.name) - signaturePriority(a.name));

  for (const signatureAsset of signatureAssets) {
    const bundleName = signatureAsset.name.slice(0, -".sig".length);
    const bundleAsset = byName.get(bundleName);
    if (!bundleAsset) {
      console.warn(`Skipping ${signatureAsset.name}: matching bundle ${bundleName} was not found`);
      continue;
    }
    const platform = platformKey(signatureAsset.name);
    if (!platform) {
      console.warn(`Skipping ${signatureAsset.name}: unknown updater platform`);
      continue;
    }
    const entry = {
      signature: fs.readFileSync(signatureAsset.path, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/${encodeURIComponent(tagName)}/${encodeURIComponent(bundleAsset.name)}`,
    };
    if (!content.platforms[platform.base]) {
      content.platforms[platform.base] = entry;
    }
    content.platforms[platform.detail] = entry;
  }

  if (!Object.keys(content.platforms).length) {
    throw new Error("No signed updater assets found. Check createUpdaterArtifacts and signing secrets.");
  }
  return content;
}

function platformKey(assetName) {
  if (
    assetName.includes("macos-aarch64") ||
    assetName.includes("aarch64-apple-darwin") ||
    assetName.includes("darwin-aarch64") ||
    assetName.includes("aarch64.dmg")
  ) {
    return { base: "darwin-aarch64", detail: "darwin-aarch64-app" };
  }
  if (
    assetName.includes("macos-x86_64") ||
    assetName.includes("x86_64-apple-darwin") ||
    assetName.includes("darwin-x86_64") ||
    assetName.includes("x64.dmg") ||
    assetName.includes("x86_64.dmg")
  ) {
    return { base: "darwin-x86_64", detail: "darwin-x86_64-app" };
  }
  if (assetName.match(/x64|x86_64|amd64/i) && assetName.match(/windows|win|setup|nsis|msi/i)) {
    const installer = assetName.match(/msi/i) ? "msi" : "nsis";
    return { base: "windows-x86_64", detail: `windows-x86_64-${installer}` };
  }
  return null;
}

function signaturePriority(name) {
  if (name.endsWith(".exe.sig") || name.endsWith(".nsis.zip.sig")) return 100;
  if (name.endsWith(".msi.sig") || name.endsWith(".msi.zip.sig")) return 90;
  if (name.endsWith(".app.tar.gz.sig")) return 100;
  return 0;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", env: process.env });
  if (result.status !== 0 && !options.allowFailure) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
  return result;
}
