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
const repo = process.env.GITHUB_REPOSITORY || "duxweb/codux";
const notesPath = process.env.RELEASE_NOTES_PATH || path.join(root, "dist", `release-notes-${version}.md`);
const artifactsDir = process.env.RELEASE_ARTIFACTS_DIR || path.join(root, "release-artifacts");
const notes = fs.existsSync(notesPath) ? fs.readFileSync(notesPath, "utf8") : `Codux ${version}`;
const manifestPath = path.join(root, "updates", channel, "latest.json");

const assets = collectAssets(artifactsDir);
if (!assets.length) {
  throw new Error(`No release assets found in ${artifactsDir}`);
}

const latestJson = buildLatestJson(assets);
const latestPath = path.join(artifactsDir, "latest.json");
fs.writeFileSync(latestPath, `${JSON.stringify(latestJson, null, 2)}\n`, "utf8");

if (!dryRun) {
  upsertRelease();

  for (const asset of assets) {
    run("gh", ["release", "upload", tagName, "--repo", repo, "--clobber", `${asset.path}#${asset.name}`]);
  }
  run("gh", ["release", "upload", tagName, "--repo", repo, "--clobber", `${latestPath}#latest.json`]);
  publishChannelManifest(latestPath);
}

console.log(
  `${dryRun ? "Prepared" : "Published"} ${assets.length} assets and updater metadata to ${repo}@${tagName} (${channel})`,
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
    const platforms = platformKey(signatureAsset.name);
    if (!platforms) {
      console.warn(`Skipping ${signatureAsset.name}: unknown updater platform`);
      continue;
    }
    const entry = {
      signature: fs.readFileSync(signatureAsset.path, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/${encodeURIComponent(tagName)}/${encodeURIComponent(bundleAsset.name)}`,
    };
    for (const platform of platforms) {
      if (!content.platforms[platform.base]) {
        content.platforms[platform.base] = entry;
      }
      content.platforms[platform.detail] = entry;
    }
  }

  if (!Object.keys(content.platforms).length) {
    throw new Error("No signed updater assets found. Check createUpdaterArtifacts and signing secrets.");
  }
  return content;
}

function platformKey(assetName) {
  if (assetName.includes("macos-universal") || assetName.includes("universal-apple-darwin")) {
    return [
      { base: "darwin-aarch64", detail: "darwin-aarch64-app" },
      { base: "darwin-x86_64", detail: "darwin-x86_64-app" },
    ];
  }
  if (
    assetName.includes("macos-aarch64") ||
    assetName.includes("aarch64-apple-darwin") ||
    assetName.includes("darwin-aarch64") ||
    assetName.includes("aarch64.dmg")
  ) {
    return [{ base: "darwin-aarch64", detail: "darwin-aarch64-app" }];
  }
  if (
    assetName.includes("macos-x86_64") ||
    assetName.includes("x86_64-apple-darwin") ||
    assetName.includes("darwin-x86_64") ||
    assetName.includes("x64.dmg") ||
    assetName.includes("x86_64.dmg")
  ) {
    return [{ base: "darwin-x86_64", detail: "darwin-x86_64-app" }];
  }
  if (assetName.match(/x64|x86_64|amd64/i) && assetName.match(/windows|win|setup|nsis|msi/i)) {
    const installer = assetName.match(/msi/i) ? "msi" : "nsis";
    return [{ base: "windows-x86_64", detail: `windows-x86_64-${installer}` }];
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

function upsertRelease() {
  const releaseFlag = channel === "beta" ? "--prerelease" : "--latest";
  const releaseExists = spawnSync("gh", ["release", "view", tagName, "--repo", repo], {
    stdio: "ignore",
    env: process.env,
  }).status === 0;
  if (releaseExists) {
    run("gh", [
      "release",
      "edit",
      tagName,
      "--repo",
      repo,
      "--title",
      `Codux ${version}`,
      "--notes-file",
      notesPath,
      releaseFlag,
    ]);
    return;
  }
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
    releaseFlag,
  ]);
}

function publishChannelManifest(sourcePath) {
  fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
  fs.copyFileSync(sourcePath, manifestPath);
  run("git", ["config", "user.name", "github-actions[bot]"]);
  run("git", ["config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"]);
  run("git", ["add", manifestPath]);
  const diff = spawnSync("git", ["diff", "--cached", "--quiet"], { stdio: "inherit", env: process.env });
  if (diff.status === 0) return;
  run("git", ["commit", "-m", `chore: update ${channel} updater manifest for ${version}`]);
  run("git", ["push", "origin", "HEAD:main"]);
}
