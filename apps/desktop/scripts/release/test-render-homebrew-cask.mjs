#!/usr/bin/env node
/* global console, process */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "codux-cask-test-"));
const caskPath = path.join(tempDir, "codux.rb");

try {
  const result = spawnSync(
    "node",
    [
      "apps/desktop/scripts/release/render-homebrew-cask.mjs",
      "v1.8.0",
      "arm-sha",
      "intel-sha",
      caskPath,
    ],
    { cwd: root, stdio: "pipe", encoding: "utf8" },
  );

  if (result.status !== 0) {
    process.stdout.write(result.stdout || "");
    process.stderr.write(result.stderr || "");
    process.exit(result.status ?? 1);
  }

  const cask = fs.readFileSync(caskPath, "utf8");
  assert.match(cask, /on_arm do/);
  assert.match(cask, /sha256 "arm-sha"/);
  assert.match(cask, /codux-macos-aarch64\.dmg/);
  assert.doesNotMatch(cask, /codux-#\{version\}-macos-aarch64\.dmg/);
  assert.match(cask, /on_intel do/);
  assert.match(cask, /sha256 "intel-sha"/);
  assert.match(cask, /codux-macos-x86_64\.dmg/);
  assert.doesNotMatch(cask, /codux-#\{version\}-macos-x86_64\.dmg/);
  assert.doesNotMatch(cask, /macos-universal-formal/);
} finally {
  fs.rmSync(tempDir, { recursive: true, force: true });
}

console.log("homebrew cask render test passed");
