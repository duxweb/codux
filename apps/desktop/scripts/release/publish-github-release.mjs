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
if (!["stable", "beta"].includes(channel)) {
  throw new Error(`RELEASE_CHANNEL must be stable or beta, got ${channel}`);
}
// Stable releases also refresh the beta manifest so beta-channel installs
// are never stranded behind the stable channel.
const manifestChannels = channel === "stable" ? ["stable", "beta"] : [channel];
const requireExistingRelease = process.env.RELEASE_REQUIRE_EXISTING === "true";
const uploadLatest = process.env.RELEASE_UPLOAD_LATEST !== "false";
const publishManifest = process.env.RELEASE_PUBLISH_MANIFEST !== "false";
const uploadSigAssets = process.env.RELEASE_UPLOAD_SIG_ASSETS === "true";
const mergeExistingLatest = process.env.RELEASE_MERGE_EXISTING_LATEST === "true";

const assets = collectAssets(artifactsDir);
if (!assets.length) {
  throw new Error(`No release assets found in ${artifactsDir}`);
}
const uploadAssets = assets.filter((asset) => shouldUploadPublicAsset(asset) || (uploadSigAssets && asset.name.endsWith(".sig")));

const latestJson = mergeExistingLatest ? mergeWithExistingLatest(buildLatestJson(assets)) : buildLatestJson(assets);
const latestPath = path.join(artifactsDir, "latest.json");
fs.writeFileSync(latestPath, `${JSON.stringify(latestJson, null, 2)}\n`, "utf8");

if (!dryRun) {
  upsertRelease();
  for (const asset of uploadAssets) {
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
  };
}

console.log(
  `${dryRun ? "Prepared" : "Published"} ${uploadAssets.length} public assets and update metadata to ${repo}@${tagName} (${channel})`,
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
  if (name.endsWith(".app.tar.gz.sig")) return ".app.tar.gz.sig";
  if (name.endsWith(".app.tar.gz")) return ".app.tar.gz";
  if (name.endsWith(".tar.gz.sig")) return ".tar.gz.sig";
  if (name.endsWith(".nsis.zip.sig")) return ".nsis.zip.sig";
  if (name.endsWith(".msi.zip.sig")) return ".msi.zip.sig";
  if (name.endsWith(".app.zip")) return ".app.zip";
  if (name.endsWith(".tar.gz")) return ".tar.gz";
  if (name.endsWith(".sha256")) return ".sha256";
  if (name.endsWith(".exe.sig")) return ".exe.sig";
  if (name.endsWith(".msi.sig")) return ".msi.sig";
  return path.extname(name);
}

function shouldUpload(asset) {
  return [
    ".dmg",
    ".exe",
    ".msi",
    ".app.tar.gz",
    ".tar.gz",
    ".app.tar.gz.sig",
    ".tar.gz.sig",
    ".nsis.zip.sig",
    ".msi.zip.sig",
    ".exe.sig",
    ".msi.sig",
  ].includes(asset.ext);
}

function shouldUploadPublicAsset(asset) {
  const lower = asset.name.toLowerCase();
  if (lower.endsWith(".sha256") || lower.endsWith(".sig")) return false;
  if (lower.endsWith(".app.zip") || lower.endsWith(".zip")) return false;
  if (lower.includes("-debug")) return false;
  if (isVersionedUserInstallerAsset(lower)) return false;
  if (asset.ext === ".exe" && !lower.endsWith("-setup.exe")) return false;
  return [".dmg", ".app.tar.gz", ".exe", ".msi"].includes(asset.ext);
}

function isVersionedUserInstallerAsset(lowerName) {
  return (
    /^codux-\d+\.\d+\.\d+(?:-[a-z0-9.]+)?-macos-(?:aarch64|x86_64|universal)\.dmg$/.test(
      lowerName,
    ) ||
    /^codux-\d+\.\d+\.\d+(?:-[a-z0-9.]+)?-windows-x86_64-setup\.exe$/.test(lowerName)
  );
}

function buildLatestJson(assets) {
  const platforms = {};
  const byName = new Map(assets.map((asset) => [asset.name, asset]));
  const signatureAssets = assets
    .filter((asset) => asset.name.endsWith(".sig"))
    .sort((left, right) => signaturePriority(right.name) - signaturePriority(left.name));

  for (const signatureAsset of signatureAssets) {
    const bundleName = signatureAsset.name.slice(0, -".sig".length);
    const bundleAsset = byName.get(bundleName);
    if (!bundleAsset) {
      console.warn(`Skipping ${signatureAsset.name}: matching bundle ${bundleName} was not found`);
      continue;
    }
    for (const platform of platformKeys(signatureAsset.name)) {
      if (platforms[platform]) continue;
      platforms[platform] = {
        signature: fs.readFileSync(signatureAsset.path, "utf8").trim(),
        url: releaseAssetUrl(bundleAsset.name),
      };
    }
  }
  if (!Object.keys(platforms).length) {
    throw new Error("No signed updater assets found. Check Tauri updater signatures.");
  }
  return {
    version,
    notes,
    pub_date: new Date().toISOString(),
    platforms,
  };
}

function releaseAssetUrl(assetName) {
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tagName)}/${encodeURIComponent(assetName)}`;
}

function platformKeys(assetName) {
  const lower = assetName.toLowerCase();
  if ((lower.includes("macos") || lower.includes("darwin")) && lower.includes("universal")) {
    return ["darwin-aarch64", "darwin-x86_64", "darwin-aarch64-app", "darwin-x86_64-app"];
  }
  if ((lower.includes("macos") || lower.includes("darwin")) && lower.includes("aarch64")) {
    return ["darwin-aarch64", "darwin-aarch64-app"];
  }
  if ((lower.includes("macos") || lower.includes("darwin")) && lower.includes("x86_64")) {
    return ["darwin-x86_64", "darwin-x86_64-app"];
  }
  if (lower.includes("windows") && lower.includes("x86_64")) {
    if (lower.includes("msi")) return ["windows-x86_64", "windows-x86_64-msi"];
    return ["windows-x86_64", "windows-x86_64-nsis"];
  }
  if (lower.includes("linux") && lower.includes("x86_64")) {
    return ["linux-x86_64"];
  }
  return [];
}

function signaturePriority(name) {
  const lower = name.toLowerCase();
  if (lower.endsWith(".app.tar.gz.sig")) return 100;
  if (lower.endsWith(".nsis.zip.sig")) return 100;
  if (lower.endsWith(".exe.sig")) return 100;
  if (lower.endsWith(".msi.zip.sig")) return 90;
  if (lower.endsWith(".msi.sig")) return 90;
  return 0;
}

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit", env: process.env });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}

function upsertRelease() {
  const releaseFlag = channel === "beta" ? "--latest=false" : "--latest";
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
  for (const manifestChannel of manifestChannels) {
    const manifestPath = path.join(root, "updates", manifestChannel, "latest.json");
    fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
    fs.writeFileSync(manifestPath, latestContent, "utf8");
    run("git", ["add", manifestPath]);
  }
  run("git", ["config", "user.name", "github-actions[bot]"]);
  run("git", ["config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"]);
  const diff = spawnSync("git", ["diff", "--cached", "--quiet"], { stdio: "inherit", env: process.env });
  if (diff.status === 0) return;
  run("git", ["commit", "-m", `chore: update ${manifestChannels.join("+")} updater manifest for ${version}`]);
  run("git", ["push", "origin", "HEAD:main"]);
}
