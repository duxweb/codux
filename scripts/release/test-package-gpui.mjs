#!/usr/bin/env node
/* global console, process */

import assert from "node:assert/strict";
import path from "node:path";

process.env.CODUX_PACKAGE_GPUI_TEST_MODE = "true";
process.env.RELEASE_STAGE_DIR = "target/release-package-test";

const { __testWindowsNsisScript } = await import("./package-gpui.mjs");

const script = __testWindowsNsisScript(
  path.join("C:", "tmp", "Codux"),
  path.join("C:", "tmp", "codux-setup.exe"),
);

assert.match(script, /!include MUI2\.nsh/);
assert.match(script, /!insertmacro MUI_PAGE_WELCOME/);
assert.match(script, /!insertmacro MUI_PAGE_DIRECTORY/);
assert.match(script, /Page custom OptionsPageCreate OptionsPageLeave/);
assert.match(script, /Create Start Menu shortcut/);
assert.match(script, /Create Desktop shortcut/);
assert.match(script, /Function EnsureCoduxCanBeUpdated/);
assert.match(script, /Codux is still running or the executable is locked/);
assert.match(script, /CreateShortcut "\$DESKTOP\\\\Codux\.lnk"/);
assert.match(script, /CreateShortcut "\$SMPROGRAMS\\\\Codux\\\\Codux\.lnk"/);
assert.doesNotMatch(script, /ShowInstDetails nevershow/);

console.log("package-gpui installer test passed");
