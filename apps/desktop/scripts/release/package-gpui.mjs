#!/usr/bin/env node
/* global console, process */

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const desktopRoot = path.join(root, "apps", "desktop");
const desktopAssetsRoot = path.join(desktopRoot, "runtime-assets");
const appName = process.env.CODUX_APP_NAME || "Codux";
const bundleId = process.env.CODUX_BUNDLE_ID || "com.duxweb.codux";
const binaryName = process.env.CODUX_BINARY_NAME || "codux";
const buildId = process.env.RELEASE_BUILD_ID || `${process.platform}-${process.arch}`;
const target = process.env.CARGO_BUILD_TARGET || "";
const profile = process.env.CARGO_PROFILE || "release";
const stageRoot = process.env.RELEASE_STAGE_DIR || "release-artifacts";
const artifactSuffix = process.env.RELEASE_ARTIFACT_SUFFIX?.trim() || "";
const outputDir = path.join(root, stageRoot, buildId);
const writeSha256Sidecars = process.env.RELEASE_WRITE_SHA256 === "true";

fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(outputDir, { recursive: true });

if (process.env.CODUX_PACKAGE_GPUI_TEST_MODE !== "true") {
  if (process.platform === "darwin") {
    packageMacos();
  } else if (process.platform === "win32") {
    packageWindows();
  } else {
    packageGenericUnix();
  }
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
  fs.copyFileSync(path.join(desktopAssetsRoot, "icons", "icon.icns"), path.join(resourcesDir, "AppIcon.icns"));
  stageRuntimeAssets(path.join(resourcesDir, "runtime-root"));
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
  createMacosDmg(appDir, dmgPath);
  if (signingIdentity && signingIdentity !== "-") {
    run("codesign", ["--force", "--timestamp", "--sign", signingIdentity, dmgPath]);
    notarizeMacosArtifact(dmgPath);
  }
  writeSha256(dmgPath);

  const updaterName = `${artifactBaseName("macos")}-updater.app.tar.gz`;
  const updaterPath = path.join(outputDir, updaterName);
  run("tar", ["-czf", updaterPath, "-C", outputDir, `${appName}.app`]);
  writeSha256(updaterPath);
  signTauriUpdaterArtifact(updaterPath);
  fs.rmSync(appDir, { recursive: true, force: true });
}

function createMacosDmg(appDir, dmgPath) {
  withTempDir("codux-dmg-", (tempDir) => {
    run("npx", [
      "--yes",
      "create-dmg@8.1.0",
      appDir,
      tempDir,
      "--overwrite",
      "--no-version-in-filename",
      "--no-code-sign",
      "--dmg-title",
      appName,
    ]);
    const generatedDmg = fs
      .readdirSync(tempDir)
      .filter((name) => name.endsWith(".dmg"))
      .map((name) => path.join(tempDir, name))[0];
    if (!generatedDmg) {
      throw new Error("create-dmg did not produce a DMG");
    }
    fs.copyFileSync(generatedDmg, dmgPath);
    fs.rmSync(generatedDmg, { force: true });
  });
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
  withTempDir("codux-windows-", (tempDir) => {
    const packageDir = path.join(tempDir, appName);
    fs.mkdirSync(packageDir, { recursive: true });
    fs.copyFileSync(exePath, path.join(packageDir, `${appName}.exe`));
    fs.copyFileSync(path.join(desktopAssetsRoot, "icons", "icon.ico"), path.join(packageDir, "icon.ico"));
    stageRuntimeAssets(path.join(packageDir, "runtime-root"));

    const installerScriptPath = path.join(tempDir, `${appName}.nsi`);
    const installerPath = path.join(outputDir, `${artifactBaseName("windows")}-setup.exe`);
    fs.writeFileSync(installerScriptPath, windowsNsisScript(packageDir, installerPath), "utf8");
    run(windowsMakensisCommand(), [installerScriptPath]);
    writeSha256(installerPath);
    signTauriUpdaterArtifact(installerPath);
  });
}

function packageGenericUnix() {
  const binaryPath = releaseBinaryPath("");
  const packageDir = path.join(outputDir, appName);
  fs.mkdirSync(packageDir, { recursive: true });
  fs.copyFileSync(binaryPath, path.join(packageDir, "codux"));
  stageRuntimeAssets(path.join(packageDir, "runtime-root"));
  const tarPath = path.join(outputDir, `${artifactBaseName("linux")}.tar.gz`);
  run("tar", ["-czf", tarPath, "-C", outputDir, appName]);
  writeSha256(tarPath);
}

function stageRuntimeAssets(destination) {
  fs.rmSync(destination, { recursive: true, force: true });
  fs.cpSync(desktopAssetsRoot, destination, {
    recursive: true,
    preserveTimestamps: true,
    verbatimSymlinks: true,
  });
  assertRuntimeBootstrapAssets(destination);
}

function assertRuntimeBootstrapAssets(runtimeRoot) {
  const required = [
    "scripts/shell-hooks/dmux-ai-hook.zsh",
    "scripts/shell-hooks/zsh/.zlogin",
    "scripts/shell-hooks/zsh/.zprofile",
    "scripts/shell-hooks/zsh/.zshenv",
    "scripts/shell-hooks/zsh/.zshrc",
    "scripts/wrappers/tool-wrapper.sh",
    "scripts/wrappers/dmux-ai-state.sh",
    "scripts/wrappers/bin/codex",
  ];
  const missing = required.filter((relativePath) => !fs.existsSync(path.join(runtimeRoot, relativePath)));
  if (missing.length > 0) {
    throw new Error(`runtime-root packaging is incomplete: ${missing.join(", ")}`);
  }
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
  const content = fs.readFileSync(path.join(desktopRoot, "Cargo.toml"), "utf8");
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
  if (writeSha256Sidecars) {
    fs.writeFileSync(`${filePath}.sha256`, `${hash}  ${path.basename(filePath)}\n`, "utf8");
  }
  console.log(`packaged ${path.basename(filePath)} sha256=${hash}`);
}

function withTempDir(prefix, callback) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  try {
    return callback(tempDir);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
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
  run("npx", ["--yes", "@tauri-apps/cli@2.0.0-rc.4", "signer", "sign", filePath], {
    env,
    shell: process.platform === "win32",
  });
}

function isReleaseBuild() {
  return Boolean(process.env.GITHUB_ACTIONS || process.env.RELEASE_REQUIRE_TAURI_SIGNATURE === "true");
}

function windowsNsisScript(packageDir, installerPath) {
  return `Unicode true
!include MUI2.nsh
!include LogicLib.nsh
!include nsDialogs.nsh

Name "${escapeNsis(appName)}"
OutFile "${escapeNsis(installerPath)}"
InstallDir "$LOCALAPPDATA\\\\Programs\\\\${escapeNsis(appName)}"
InstallDirRegKey HKCU "Software\\\\${escapeNsis(appName)}" "InstallDir"
RequestExecutionLevel user
SilentInstall normal
ShowInstDetails show
ShowUninstDetails show

!define MUI_ABORTWARNING
!define MUI_ICON "${escapeNsis(path.join(desktopAssetsRoot, "icons", "icon.ico"))}"
!define MUI_UNICON "${escapeNsis(path.join(desktopAssetsRoot, "icons", "icon.ico"))}"

Var CreateDesktopShortcut
Var CreateStartMenuShortcut
Var DesktopShortcutCheckbox
Var StartMenuShortcutCheckbox

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
Page custom OptionsPageCreate OptionsPageLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Function .onInit
  StrCpy $CreateDesktopShortcut 1
  StrCpy $CreateStartMenuShortcut 1
FunctionEnd

Function OptionsPageCreate
  nsDialogs::Create 1018
  Pop $0
  \${If} $0 == error
    Abort
  \${EndIf}

  \${NSD_CreateLabel} 0 0 100% 24u "Choose which shortcuts Codux should create."
  Pop $0
  \${NSD_CreateCheckbox} 0 34u 100% 12u "Create Start Menu shortcut"
  Pop $StartMenuShortcutCheckbox
  \${If} $CreateStartMenuShortcut == 1
    \${NSD_Check} $StartMenuShortcutCheckbox
  \${EndIf}
  \${NSD_CreateCheckbox} 0 56u 100% 12u "Create Desktop shortcut"
  Pop $DesktopShortcutCheckbox
  \${If} $CreateDesktopShortcut == 1
    \${NSD_Check} $DesktopShortcutCheckbox
  \${EndIf}

  nsDialogs::Show
FunctionEnd

Function OptionsPageLeave
  \${NSD_GetState} $StartMenuShortcutCheckbox $CreateStartMenuShortcut
  \${NSD_GetState} $DesktopShortcutCheckbox $CreateDesktopShortcut
FunctionEnd

Function EnsureCoduxCanBeUpdated
  IfFileExists "$INSTDIR\\\\${escapeNsis(appName)}.exe" 0 done
  ClearErrors
  FileOpen $0 "$INSTDIR\\\\${escapeNsis(appName)}.exe" a
  IfErrors locked unlocked
  unlocked:
    FileClose $0
    Goto done
  locked:
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "${escapeNsis(appName)} is still running or the executable is locked.$\\r$\\n$\\r$\\nClose ${escapeNsis(appName)} and click Retry to continue." IDRETRY retry
    Abort
  retry:
    Call EnsureCoduxCanBeUpdated
  done:
FunctionEnd

Section "Install"
  Call EnsureCoduxCanBeUpdated
  SetOutPath "$INSTDIR"
  File /r "${escapeNsis(packageDir)}\\\\*"
  \${If} $CreateStartMenuShortcut == 1
    CreateDirectory "$SMPROGRAMS\\\\${escapeNsis(appName)}"
    CreateShortcut "$SMPROGRAMS\\\\${escapeNsis(appName)}\\\\${escapeNsis(appName)}.lnk" "$INSTDIR\\\\${escapeNsis(appName)}.exe"
  \${EndIf}
  \${If} $CreateDesktopShortcut == 1
    CreateShortcut "$DESKTOP\\\\${escapeNsis(appName)}.lnk" "$INSTDIR\\\\${escapeNsis(appName)}.exe"
  \${EndIf}
  WriteRegStr HKCU "Software\\\\${escapeNsis(appName)}" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\\\\Uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\\\\${escapeNsis(appName)}\\\\${escapeNsis(appName)}.lnk"
  RMDir "$SMPROGRAMS\\\\${escapeNsis(appName)}"
  Delete "$DESKTOP\\\\${escapeNsis(appName)}.lnk"
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

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    env: options.env || process.env,
    shell: options.shell || false,
  });
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

export function __testWindowsNsisScript(packageDir, installerPath) {
  return windowsNsisScript(packageDir, installerPath);
}

export function __testStageRuntimeAssets(destination) {
  stageRuntimeAssets(destination);
}
