#!/usr/bin/env node
/* global console, process */

import crypto from "node:crypto";
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
const requireExistingRelease = process.env.RELEASE_REQUIRE_EXISTING === "true";
const uploadLatest = process.env.RELEASE_UPLOAD_LATEST !== "false";
const publishManifest = process.env.RELEASE_PUBLISH_MANIFEST !== "false";
const mergeExistingLatest = process.env.RELEASE_MERGE_EXISTING_LATEST === "true";

const assets = collectAssets(artifactsDir);
if (!assets.length) {
  throw new Error(`No release assets found in ${artifactsDir}`);
}

const latestJson = mergeExistingLatest ? mergeWithExistingLatest(buildLatestJson(assets)) : buildLatestJson(assets);
const latestPath = path.join(artifactsDir, "latest.json");
fs.writeFileSync(latestPath, `${JSON.stringify(latestJson, null, 2)}\n`, "utf8");

if (!dryRun) {
  upsertRelease();
  for (const asset of assets) {
    run("gh", ["release", "upload", tagName, "--repo", repo, "--clobber", `${asset.path}#${asset.name}`]);
  }
  if (uploadLatest) {
    run("gh", ["release", "upload", tagName, "--repo", repo, "--clobber", `${latestPath}#latest.json`]);
  }
  if (publishManifest) {
    publishChannelManifest(latestPath);
  }
}

function mergeWithExistingLatest(next) {
  if (dryRun) return next;
  const tempDir = path.join(artifactsDir, `.existing-latest-${Date.now()}`);
  fs.rmSync(tempDir, { recursive: true, force: true });
  fs.mkdirSync(tempDir, { recursive: true });
  const downloaded = spawnSync(
    "gh",
    ["release", "download", tagName, "--repo", repo, "--pattern", "latest.json", "--dir", tempDir],
    { stdio: "ignore", env: process.env },
  );
  if (downloaded.status !== 0) {
    fs.rmSync(tempDir, { recursive: true, force: true });
    return next;
  }
  const existingPath = path.join(tempDir, "latest.json");
  if (!fs.existsSync(existingPath)) {
    fs.rmSync(tempDir, { recursive: true, force: true });
    return next;
  }
  const existing = JSON.parse(fs.readFileSync(existingPath, "utf8"));
  fs.rmSync(tempDir, { recursive: true, force: true });
  return {
    ...existing,
    version: next.version,
    notes: next.notes,
    pub_date: next.pub_date,
    platforms: {
      ...(existing.platforms || {}),
      ...next.platforms,
    },
    downloadUrl: next.downloadUrl || existing.downloadUrl,
    checksum: next.checksum || existing.checksum,
  };
}

console.log(
  `${dryRun ? "Prepared" : "Published"} ${assets.length} assets and update metadata to ${repo}@${tagName} (${channel})`,
);

function requiredEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function collectAssets(dir) {
  return walk(dir)
    .filter((file) => !file.endsWith("latest.json"))
    .filter((file) => !file.endsWith(".blockmap"))
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
  if (name.endsWith(".app.zip")) return ".app.zip";
  if (name.endsWith(".tar.gz")) return ".tar.gz";
  if (name.endsWith(".sha256")) return ".sha256";
  return path.extname(name);
}

function shouldUpload(asset) {
  return [".dmg", ".app.zip", ".zip", ".exe", ".msi", ".tar.gz", ".sha256"].includes(asset.ext);
}

function buildLatestJson(assets) {
  const downloadAsset = preferredDownloadAsset(assets);
  if (!downloadAsset) {
    throw new Error("No downloadable release asset found.");
  }
  const platforms = {};
  for (const asset of assets.filter((item) => item.ext !== ".sha256").sort(platformManifestAssetSort)) {
    for (const platform of platformKeys(asset.name)) {
      if (platforms[platform]) continue;
      platforms[platform] = {
        url: releaseAssetUrl(asset.name),
        checksum: sha256ForAsset(asset),
        signature: "",
      };
    }
  }
  return {
    version,
    notes,
    pub_date: new Date().toISOString(),
    downloadUrl: releaseAssetUrl(downloadAsset.name),
    checksum: sha256ForAsset(downloadAsset),
    automaticInstallSupported: true,
    platforms,
  };
}

function platformManifestAssetSort(left, right) {
  const leftScore = platformManifestAssetScore(left.name);
  const rightScore = platformManifestAssetScore(right.name);
  return leftScore - rightScore;
}

function platformManifestAssetScore(name) {
  const lower = name.toLowerCase();
  if (lower.includes("macos") && lower.endsWith(".app.zip")) return 0;
  if (lower.includes("windows") && lower.endsWith(".exe")) return 0;
  if (lower.includes("windows") && lower.endsWith(".msi")) return 1;
  if (lower.includes("windows") && lower.endsWith(".zip")) return 10;
  if (lower.endsWith(".tar.gz")) return 0;
  if (lower.endsWith(".dmg")) return 10;
  return 20;
}

function preferredDownloadAsset(assets) {
  const candidates = assets.filter((asset) => asset.ext !== ".sha256");
  return (
    candidates.find((asset) => asset.name.endsWith(".dmg")) ||
    candidates.find((asset) => asset.name.toLowerCase().includes("windows") && asset.name.endsWith(".exe")) ||
    candidates.find((asset) => asset.name.endsWith(".zip")) ||
    candidates[0]
  );
}

function releaseAssetUrl(assetName) {
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tagName)}/${encodeURIComponent(assetName)}`;
}

function sha256ForAsset(asset) {
  if (asset.ext === ".sha256") return "";
  const sidecar = `${asset.path}.sha256`;
  if (fs.existsSync(sidecar)) {
    return fs.readFileSync(sidecar, "utf8").trim().split(/\s+/)[0] || "";
  }
  return crypto.createHash("sha256").update(fs.readFileSync(asset.path)).digest("hex");
}

function platformKeys(assetName) {
  const lower = assetName.toLowerCase();
  if (lower.includes("macos") && lower.includes("universal")) {
    return ["darwin-aarch64", "darwin-x86_64", "darwin-aarch64-app", "darwin-x86_64-app"];
  }
  if (lower.includes("macos") && lower.includes("aarch64")) {
    return ["darwin-aarch64", "darwin-aarch64-app"];
  }
  if (lower.includes("macos") && lower.includes("x86_64")) {
    return ["darwin-x86_64", "darwin-x86_64-app"];
  }
  if (lower.includes("windows") && lower.includes("x86_64")) {
    return ["windows-x86_64"];
  }
  if (lower.includes("linux") && lower.includes("x86_64")) {
    return ["linux-x86_64"];
  }
  return [];
}

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit", env: process.env });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}

function upsertRelease() {
  const releaseFlag = channel === "beta" ? "--prerelease" : "--latest";
  const releaseExists =
    spawnSync("gh", ["release", "view", tagName, "--repo", repo], {
      stdio: "ignore",
      env: process.env,
    }).status === 0;
  if (!releaseExists && requireExistingRelease) {
    throw new Error(`Release ${tagName} does not exist.`);
  }
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
  const latestContent = fs.readFileSync(sourcePath, "utf8");
  run("git", ["fetch", "origin", "main"]);
  run("git", ["checkout", "-B", "main", "origin/main"]);
  fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
  fs.writeFileSync(manifestPath, latestContent, "utf8");
  run("git", ["config", "user.name", "github-actions[bot]"]);
  run("git", ["config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"]);
  run("git", ["add", manifestPath]);
  const diff = spawnSync("git", ["diff", "--cached", "--quiet"], { stdio: "inherit", env: process.env });
  if (diff.status === 0) return;
  run("git", ["commit", "-m", `chore: update ${channel} updater manifest for ${version}`]);
  run("git", ["push", "origin", "HEAD:main"]);
}
