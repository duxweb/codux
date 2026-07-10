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
// Stable, version-less copies of the user-facing installers so
// `releases/latest/download/<stable-name>` always points at the newest build
// (no per-release README edits). Only for real release builds, not debug.
const writeStableAlias = !artifactSuffix && process.env.RELEASE_WRITE_STABLE_ALIAS !== "false";

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
  writeStableAliasCopy(dmgPath, `${stableArtifactBaseName("macos")}.dmg`);

  const updaterName = `${artifactBaseName("macos")}-updater.app.tar.gz`;
  const updaterPath = path.join(outputDir, updaterName);
  run("tar", ["-czf", updaterPath, "-C", outputDir, `${appName}.app`]);
  writeSha256(updaterPath);
  signTauriUpdaterArtifact(updaterPath);
  writeStableAliasCopy(updaterPath, `${stableArtifactBaseName("macos")}-updater.app.tar.gz`);
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
  const helperPath = releaseBinaryPath(".exe", "codux-wrapper-helper");
  withTempDir("codux-windows-", (tempDir) => {
    const packageDir = path.join(tempDir, appName);
    fs.mkdirSync(packageDir, { recursive: true });
    fs.copyFileSync(exePath, path.join(packageDir, `${appName}.exe`));
    fs.copyFileSync(path.join(desktopAssetsRoot, "icons", "icon.ico"), path.join(packageDir, "icon.ico"));
    stageRuntimeAssets(path.join(packageDir, "runtime-root"));
    fs.copyFileSync(
      helperPath,
      path.join(packageDir, "runtime-root", "scripts", "wrappers", "codux-wrapper-helper.exe"),
    );
    if (process.env.CODUX_TEST_PACKAGE_DIR) {
      fs.rmSync(process.env.CODUX_TEST_PACKAGE_DIR, { recursive: true, force: true });
      fs.cpSync(packageDir, process.env.CODUX_TEST_PACKAGE_DIR, {
        recursive: true,
        dereference: true,
      });
    }

    const installerScriptPath = path.join(tempDir, `${appName}.nsi`);
    const installerPath = path.join(outputDir, `${artifactBaseName("windows")}-setup.exe`);
    // BOM so makensis reads the localized LangStrings as UTF-8.
    fs.writeFileSync(installerScriptPath, "\ufeff" + windowsNsisScript(packageDir, installerPath), "utf8");
    if (process.env.CODUX_TEST_SKIP_MAKENSIS === "true") {
      fs.writeFileSync(installerPath, "");
    } else {
      run(windowsMakensisCommand(), [installerScriptPath]);
    }
    writeSha256(installerPath);
    signTauriUpdaterArtifact(installerPath);
    writeStableAliasCopy(installerPath, `${stableArtifactBaseName("windows")}-setup.exe`);
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
    dereference: true,
    preserveTimestamps: true,
  });
  materializeSymlinks(destination);
  assertRuntimeBootstrapAssets(destination);
}

function materializeSymlinks(rootPath) {
  for (const entry of fs.readdirSync(rootPath, { withFileTypes: true })) {
    const entryPath = path.join(rootPath, entry.name);
    if (entry.isSymbolicLink()) {
      materializeSymlink(entryPath);
      continue;
    }
    if (entry.isDirectory()) {
      materializeSymlinks(entryPath);
    }
  }
}

function materializeSymlink(linkPath) {
  const targetPath = fs.realpathSync(linkPath);
  const targetStat = fs.statSync(targetPath);
  fs.rmSync(linkPath, { recursive: true, force: true });
  if (targetStat.isDirectory()) {
    fs.cpSync(targetPath, linkPath, {
      recursive: true,
      dereference: true,
      preserveTimestamps: true,
    });
    materializeSymlinks(linkPath);
    return;
  }
  fs.copyFileSync(targetPath, linkPath);
  fs.chmodSync(linkPath, targetStat.mode);
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
    "scripts/wrappers/codux-ssh.ps1",
    "scripts/wrappers/codux-db.ps1",
    "scripts/wrappers/bin/codex",
    "scripts/wrappers/bin/codux-ssh",
    "scripts/wrappers/bin/codux-ssh.ps1",
    "scripts/wrappers/bin/codux-db",
    "scripts/wrappers/bin/codux-db.ps1",
  ];
  const missing = required.filter((relativePath) => !fs.existsSync(path.join(runtimeRoot, relativePath)));
  if (missing.length > 0) {
    throw new Error(`runtime-root packaging is incomplete: ${missing.join(", ")}`);
  }
}

function releaseBinaryPath(extension, name = binaryName) {
  const releaseBinaryOverrideDir = process.env.CODUX_RELEASE_BINARY_DIR?.trim() || "";
  if (releaseBinaryOverrideDir) {
    const binaryPath = path.join(releaseBinaryOverrideDir, `${name}${extension}`);
    if (!fs.existsSync(binaryPath)) {
      throw new Error(`Built binary not found: ${binaryPath}`);
    }
    return binaryPath;
  }
  const segments = [root, "target"];
  if (target) segments.push(target);
  segments.push(profile, `${name}${extension}`);
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

function stableArtifactBaseName(platform) {
  return `codux-${platform}-${targetArchLabel()}`;
}

function writeStableAliasCopy(sourcePath, stableName) {
  if (!writeStableAlias) return;
  const stablePath = path.join(outputDir, stableName);
  fs.copyFileSync(sourcePath, stablePath);
  console.log(`aliased ${path.basename(sourcePath)} -> ${stableName}`);
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
  const version = readCargoVersion();
  const fileVersion = `${version.match(/^(\d+)\.(\d+)\.(\d+)/)?.slice(1).join(".") || "0.0.0"}.0`;
  const headerBitmap = path.join(desktopRoot, "scripts", "release", "installer-header.bmp");
  const estimatedSizeKB = directorySizeKB(packageDir);
  return `Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma
!include MUI2.nsh
!include LogicLib.nsh
!include StrFunc.nsh
\${StrRep}

Name "${escapeNsis(appName)}"
OutFile "${escapeNsis(installerPath)}"
InstallDir "$LOCALAPPDATA\\Programs\\${escapeNsis(appName)}"
InstallDirRegKey HKCU "Software\\${escapeNsis(appName)}" "InstallDir"
RequestExecutionLevel user
SilentInstall normal
ShowInstDetails nevershow
ShowUninstDetails nevershow
AutoCloseWindow true
BrandingText "${escapeNsis(appName)} ${escapeNsis(version)}"

VIProductVersion "${fileVersion}"
VIAddVersionKey /LANG=0 "ProductName" "${escapeNsis(appName)}"
VIAddVersionKey /LANG=0 "ProductVersion" "${escapeNsis(version)}"
VIAddVersionKey /LANG=0 "FileVersion" "${fileVersion}"
VIAddVersionKey /LANG=0 "FileDescription" "${escapeNsis(appName)} Installer"
VIAddVersionKey /LANG=0 "CompanyName" "duxweb"
VIAddVersionKey /LANG=0 "LegalCopyright" "duxweb"

!define UNINSTALL_KEY "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${escapeNsis(appName)}"

!define MUI_ABORTWARNING
!define MUI_ICON "${escapeNsis(path.join(desktopAssetsRoot, "icons", "icon.ico"))}"
!define MUI_UNICON "${escapeNsis(path.join(desktopAssetsRoot, "icons", "icon.ico"))}"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "${escapeNsis(headerBitmap)}"

Var FreshInstall

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"

SetFont /LANG=\${LANG_ENGLISH} "Segoe UI" 9
SetFont /LANG=\${LANG_SIMPCHINESE} "Microsoft YaHei UI" 9

LangString MsgAppRunning \${LANG_ENGLISH} "${escapeNsis(appName)} is still running or the executable is locked.$\\r$\\n$\\r$\\nClose ${escapeNsis(appName)} and click Retry to continue."
LangString MsgAppRunning \${LANG_SIMPCHINESE} "${escapeNsis(appName)} 仍在运行或程序文件被占用。$\\r$\\n$\\r$\\n请退出 ${escapeNsis(appName)} 后点击“重试”继续。"
LangString MsgRemoveData \${LANG_ENGLISH} "Also remove ${escapeNsis(appName)} user data (settings, AI statistics, memory)?"
LangString MsgRemoveData \${LANG_SIMPCHINESE} "是否同时删除 ${escapeNsis(appName)} 用户数据（设置、AI 统计、记忆）？"

Function .onInit
  ReadRegStr $0 HKCU "Software\\${escapeNsis(appName)}" "InstallDir"
  \${If} $0 == ""
    StrCpy $FreshInstall 1
  \${Else}
    StrCpy $FreshInstall 0
  \${EndIf}
  ; Older installers stored InstallDir with doubled backslashes; collapse them (keep a UNC lead).
  StrCpy $0 $INSTDIR 1
  StrCpy $1 $INSTDIR "" 1
  \${StrRep} $1 $1 "\\\\" "\\"
  StrCpy $INSTDIR "$0$1"
FunctionEnd

Function .onInstSuccess
  ; Silent updates relaunch via the updater's helper script instead.
  IfSilent +2
  Exec '"$INSTDIR\\${escapeNsis(appName)}.exe"'
FunctionEnd

Function EnsureCoduxCanBeUpdated
  StrCpy $1 0
  check:
  IfFileExists "$INSTDIR\\${escapeNsis(appName)}.exe" 0 done
  ClearErrors
  FileOpen $0 "$INSTDIR\\${escapeNsis(appName)}.exe" a
  IfErrors locked
  FileClose $0
  Goto done
  locked:
  \${If} \${Silent}
    IntOp $1 $1 + 1
    \${If} $1 > 40
      Abort
    \${EndIf}
    Sleep 500
    Goto check
  \${Else}
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(MsgAppRunning)" IDRETRY check
    Abort
  \${EndIf}
  done:
FunctionEnd

Section "Install"
  Call EnsureCoduxCanBeUpdated
  SetOutPath "$INSTDIR"
  File /r "${escapeNsis(packageDir)}\\*"
  CreateDirectory "$SMPROGRAMS\\${escapeNsis(appName)}"
  CreateShortcut "$SMPROGRAMS\\${escapeNsis(appName)}\\${escapeNsis(appName)}.lnk" "$INSTDIR\\${escapeNsis(appName)}.exe"
  \${If} $FreshInstall == 1
    CreateShortcut "$DESKTOP\\${escapeNsis(appName)}.lnk" "$INSTDIR\\${escapeNsis(appName)}.exe"
  \${EndIf}
  WriteRegStr HKCU "Software\\${escapeNsis(appName)}" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\\Uninstall.exe"
  WriteRegStr HKCU "\${UNINSTALL_KEY}" "DisplayName" "${escapeNsis(appName)}"
  WriteRegStr HKCU "\${UNINSTALL_KEY}" "DisplayVersion" "${escapeNsis(version)}"
  WriteRegStr HKCU "\${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\\${escapeNsis(appName)}.exe"
  WriteRegStr HKCU "\${UNINSTALL_KEY}" "Publisher" "duxweb"
  WriteRegStr HKCU "\${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "\${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\\Uninstall.exe"'
  WriteRegStr HKCU "\${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\\Uninstall.exe" /S'
  WriteRegDWORD HKCU "\${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "\${UNINSTALL_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "\${UNINSTALL_KEY}" "EstimatedSize" ${estimatedSizeKB}
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\\${escapeNsis(appName)}\\${escapeNsis(appName)}.lnk"
  RMDir "$SMPROGRAMS\\${escapeNsis(appName)}"
  Delete "$DESKTOP\\${escapeNsis(appName)}.lnk"
  Delete "$INSTDIR\\${escapeNsis(appName)}.exe"
  Delete "$INSTDIR\\icon.ico"
  RMDir /r "$INSTDIR\\runtime-root"
  Delete "$INSTDIR\\Uninstall.exe"
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "$(MsgRemoveData)" /SD IDNO IDNO keepdata
  RMDir /r "$INSTDIR\\Data"
  keepdata:
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\\${escapeNsis(appName)}"
  DeleteRegKey HKCU "\${UNINSTALL_KEY}"
SectionEnd
`;
}

function directorySizeKB(dir) {
  try {
    let bytes = 0;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true, recursive: true })) {
      if (entry.isFile()) bytes += fs.statSync(path.join(entry.parentPath ?? entry.path, entry.name)).size;
    }
    return Math.ceil(bytes / 1024);
  } catch {
    return 0;
  }
}

function escapeNsis(value) {
  return String(value).replaceAll('"', '$\\"');
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

export function __testPackageWindows() {
  packageWindows();
}
