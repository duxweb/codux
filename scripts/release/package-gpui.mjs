#!/usr/bin/env node
/* global console, process */

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const appName = process.env.CODUX_APP_NAME || "Codux";
const bundleId = process.env.CODUX_BUNDLE_ID || "com.duxweb.codux";
const binaryName = process.env.CODUX_BINARY_NAME || "codux";
const buildId = process.env.RELEASE_BUILD_ID || `${process.platform}-${process.arch}`;
const target = process.env.CARGO_BUILD_TARGET || "";
const profile = process.env.CARGO_PROFILE || "release";
const stageRoot = process.env.RELEASE_STAGE_DIR || "release-artifacts";
const artifactSuffix = process.env.RELEASE_ARTIFACT_SUFFIX?.trim() || "";
const outputDir = path.join(root, stageRoot, buildId);

fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(outputDir, { recursive: true });

if (process.platform === "darwin") {
  packageMacos();
} else if (process.platform === "win32") {
  packageWindows();
} else {
  packageGenericUnix();
}

function packageMacos() {
  const binaryPath = releaseBinaryPath("");
  const appDir = path.join(outputDir, `${appName}.app`);
  const contentsDir = path.join(appDir, "Contents");
  const macosDir = path.join(contentsDir, "MacOS");
  const resourcesDir = path.join(contentsDir, "Resources");
  fs.mkdirSync(macosDir, { recursive: true });
  fs.mkdirSync(resourcesDir, { recursive: true });
  fs.copyFileSync(binaryPath, path.join(macosDir, appName));
  fs.copyFileSync(path.join(root, "runtime-assets", "icons", "icon.icns"), path.join(resourcesDir, "AppIcon.icns"));
  fs.writeFileSync(path.join(contentsDir, "Info.plist"), macosInfoPlist(), "utf8");

  const signingIdentity = macosSigningIdentity();
  if (signingIdentity) {
    codesignMacos(appDir, signingIdentity);
    if (signingIdentity !== "-") {
      notarizeMacosApp(appDir);
    }
  }

  const dmgName = `${artifactBaseName("macos")}.dmg`;
  const dmgPath = path.join(outputDir, dmgName);
  run("hdiutil", ["create", "-volname", appName, "-srcfolder", appDir, "-ov", "-format", "UDZO", dmgPath]);
  if (signingIdentity && signingIdentity !== "-") {
    run("codesign", ["--force", "--timestamp", "--sign", signingIdentity, dmgPath]);
    notarizeMacosArtifact(dmgPath);
  }
  writeSha256(dmgPath);

  const zipName = `${artifactBaseName("macos")}.app.zip`;
  run("ditto", ["-c", "-k", "--keepParent", appDir, path.join(outputDir, zipName)]);
  writeSha256(path.join(outputDir, zipName));

  const updaterName = `${artifactBaseName("macos")}-updater.app.tar.gz`;
  const updaterPath = path.join(outputDir, updaterName);
  run("tar", ["-czf", updaterPath, "-C", outputDir, `${appName}.app`]);
  writeSha256(updaterPath);
  signTauriUpdaterArtifact(updaterPath);
}

function notarizeMacosApp(appDir) {
  if (!appleNotaryConfigured()) return;
  const notaryZip = path.join(outputDir, `${appName}-notary.zip`);
  run("ditto", ["-c", "-k", "--keepParent", appDir, notaryZip]);
  notarizeMacosArtifact(notaryZip);
  fs.rmSync(notaryZip, { force: true });
  run("xcrun", ["stapler", "staple", appDir]);
}

function macosSigningIdentity() {
  const explicit = process.env.MACOS_SIGNING_IDENTITY?.trim() || process.env.APPLE_SIGNING_IDENTITY?.trim();
  if (explicit) return explicit;
  if (process.env.MACOS_ADHOC_SIGN === "false") return "";
  return "-";
}

function codesignMacos(appDir, signingIdentity) {
  const args = ["--force", "--deep"];
  if (signingIdentity !== "-") {
    args.push("--options", "runtime", "--timestamp");
  }
  args.push("--sign", signingIdentity, appDir);
  run("codesign", args);
}

function notarizeMacosArtifact(artifactPath) {
  if (!appleNotaryConfigured()) return;
  run("xcrun", [
    "notarytool",
    "submit",
    artifactPath,
    "--apple-id",
    process.env.APPLE_ID.trim(),
    "--password",
    process.env.APPLE_PASSWORD.trim(),
    "--team-id",
    process.env.APPLE_TEAM_ID.trim(),
    "--wait",
  ]);
  if (artifactPath.endsWith(".dmg")) {
    run("xcrun", ["stapler", "staple", artifactPath]);
  }
}

function appleNotaryConfigured() {
  return Boolean(
    process.env.APPLE_ID?.trim() && process.env.APPLE_PASSWORD?.trim() && process.env.APPLE_TEAM_ID?.trim(),
  );
}

function packageWindows() {
  const exePath = releaseBinaryPath(".exe");
  const packageDir = path.join(outputDir, appName);
  fs.mkdirSync(packageDir, { recursive: true });
  fs.copyFileSync(exePath, path.join(packageDir, `${appName}.exe`));
  fs.copyFileSync(path.join(root, "runtime-assets", "icons", "icon.ico"), path.join(packageDir, "icon.ico"));
  const zipPath = path.join(outputDir, `${artifactBaseName("windows")}.zip`);
  run("powershell", [
    "-NoProfile",
    "-Command",
    `Compress-Archive -Path '${packageDir.replaceAll("'", "''")}\\*' -DestinationPath '${zipPath.replaceAll("'", "''")}' -Force`,
  ]);
  writeSha256(zipPath);

  const installerScriptPath = path.join(outputDir, `${appName}.nsi`);
  const installerPath = path.join(outputDir, `${artifactBaseName("windows")}-setup.exe`);
  fs.writeFileSync(installerScriptPath, windowsNsisScript(packageDir, installerPath), "utf8");
  run(windowsMakensisCommand(), [installerScriptPath]);
  writeSha256(installerPath);
  signTauriUpdaterArtifact(installerPath);
}

function packageGenericUnix() {
  const binaryPath = releaseBinaryPath("");
  const packageDir = path.join(outputDir, appName);
  fs.mkdirSync(packageDir, { recursive: true });
  fs.copyFileSync(binaryPath, path.join(packageDir, "codux"));
  const tarPath = path.join(outputDir, `${artifactBaseName("linux")}.tar.gz`);
  run("tar", ["-czf", tarPath, "-C", outputDir, appName]);
  writeSha256(tarPath);
}

function releaseBinaryPath(extension) {
  const segments = [root, "target"];
  if (target) segments.push(target);
  segments.push(profile, `${binaryName}${extension}`);
  const binaryPath = path.join(...segments);
  if (!fs.existsSync(binaryPath)) {
    throw new Error(`Built binary not found: ${binaryPath}`);
  }
  return binaryPath;
}

function artifactBaseName(platform) {
  const version = readCargoVersion();
  const arch = targetArchLabel();
  return `codux-${version}-${platform}-${arch}${artifactSuffix}`;
}

function targetArchLabel() {
  if (target.includes("universal-apple-darwin")) return "universal";
  if (target.includes("aarch64")) return "aarch64";
  if (target.includes("x86_64")) return "x86_64";
  if (process.arch === "arm64") return "aarch64";
  if (process.arch === "x64") return "x86_64";
  return process.arch || os.arch();
}

function readCargoVersion() {
  const content = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  return content.match(/^version = "(.+)"$/m)?.[1] || "0.0.0";
}

function macosInfoPlist() {
  const version = readCargoVersion();
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${escapeXml(appName)}</string>
  <key>CFBundleExecutable</key>
  <string>${escapeXml(appName)}</string>
  <key>CFBundleIdentifier</key>
  <string>${escapeXml(bundleId)}</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${escapeXml(appName)}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${escapeXml(version)}</string>
  <key>CFBundleVersion</key>
  <string>${escapeXml(bundleVersion(version))}</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
`;
}

function bundleVersion(version) {
  const match = version.match(/^(\d+)\.(\d+)\.(\d+)/);
  return match ? `${match[1]}.${match[2]}.${match[3]}` : version;
}

function escapeXml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

function writeSha256(filePath) {
  const hash = crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
  fs.writeFileSync(`${filePath}.sha256`, `${hash}  ${path.basename(filePath)}\n`, "utf8");
  console.log(`packaged ${path.basename(filePath)} sha256=${hash}`);
}

function signTauriUpdaterArtifact(filePath) {
  const privateKey = process.env.TAURI_PRIVATE_KEY?.trim() || process.env.TAURI_SIGNING_PRIVATE_KEY?.trim();
  if (!privateKey) {
    if (isReleaseBuild()) {
      throw new Error(`Tauri updater signature key is required for ${path.basename(filePath)}`);
    }
    console.warn(`skipping Tauri updater signature for ${path.basename(filePath)}: signing key is not configured`);
    return;
  }
  const password =
    process.env.TAURI_PRIVATE_KEY_PASSWORD ?? process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "";
  const env = {
    ...process.env,
    TAURI_PRIVATE_KEY: privateKey,
    TAURI_PRIVATE_KEY_PASSWORD: password,
  };
  run(
    nodeBin("npx"),
    ["--yes", "@tauri-apps/cli@2.0.0-rc.4", "signer", "sign", filePath],
    { env },
  );
}

function isReleaseBuild() {
  return Boolean(process.env.GITHUB_ACTIONS || process.env.RELEASE_REQUIRE_TAURI_SIGNATURE === "true");
}

function windowsNsisScript(packageDir, installerPath) {
  return `Unicode true
Name "${escapeNsis(appName)}"
OutFile "${escapeNsis(installerPath)}"
InstallDir "$LOCALAPPDATA\\\\Programs\\\\${escapeNsis(appName)}"
InstallDirRegKey HKCU "Software\\\\${escapeNsis(appName)}" "InstallDir"
RequestExecutionLevel user
SilentInstall normal
ShowInstDetails nevershow

Section "Install"
  SetOutPath "$INSTDIR"
  File /r "${escapeNsis(packageDir)}\\\\*"
  CreateDirectory "$SMPROGRAMS\\\\${escapeNsis(appName)}"
  CreateShortcut "$SMPROGRAMS\\\\${escapeNsis(appName)}\\\\${escapeNsis(appName)}.lnk" "$INSTDIR\\\\${escapeNsis(appName)}.exe"
  WriteRegStr HKCU "Software\\\\${escapeNsis(appName)}" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\\\\Uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\\\\${escapeNsis(appName)}\\\\${escapeNsis(appName)}.lnk"
  RMDir "$SMPROGRAMS\\\\${escapeNsis(appName)}"
  Delete "$INSTDIR\\\\Uninstall.exe"
  Delete "$INSTDIR\\\\${escapeNsis(appName)}.exe"
  Delete "$INSTDIR\\\\icon.ico"
  DeleteRegKey HKCU "Software\\\\${escapeNsis(appName)}"
  RMDir "$INSTDIR"
SectionEnd
`;
}

function escapeNsis(value) {
  return String(value).replaceAll("\\", "\\\\").replaceAll('"', '$\\"');
}

function windowsMakensisCommand() {
  if (process.platform !== "win32") return "makensis";
  const candidates = [
    process.env.MAKENSIS_PATH,
    process.env.NSIS_HOME && path.join(process.env.NSIS_HOME, "makensis.exe"),
    process.env.ProgramFiles && path.join(process.env.ProgramFiles, "NSIS", "makensis.exe"),
    process.env["ProgramFiles(x86)"] && path.join(process.env["ProgramFiles(x86)"], "NSIS", "makensis.exe"),
    process.env.ChocolateyInstall && path.join(process.env.ChocolateyInstall, "bin", "makensis.exe"),
    "makensis",
  ].filter(Boolean);
  return candidates.find((candidate) => candidate === "makensis" || fs.existsSync(candidate)) || "makensis";
}

function nodeBin(command) {
  return process.platform === "win32" ? `${command}.cmd` : command;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", env: options.env || process.env });
  if (result.status !== 0) {
    const details = [
      `exit code ${result.status}`,
      result.signal ? `signal ${result.signal}` : "",
      result.error ? `error ${result.error.message}` : "",
    ]
      .filter(Boolean)
      .join(", ");
    throw new Error(`${command} ${args.join(" ")} failed with ${details}`);
  }
}
