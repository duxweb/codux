#!/usr/bin/env node
/* global console, process */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "codux-prepare-release-test-"));
const notesPath = path.join(tempDir, "release-notes.md");

const result = spawnSync("node", ["apps/desktop/scripts/release/prepare-release.mjs", "v1.8.0", "--dry-run"], {
  cwd: root,
  env: {
    ...process.env,
    RELEASE_NOTES_PATH: notesPath,
  },
  encoding: "utf8",
});

assert.equal(result.status, 0, result.stderr || result.stdout);
const notes = fs.readFileSync(notesPath, "utf8");
assert.match(notes, /## Downloads \/ 下载说明/);
assert.match(notes, /\| File \/ 文件 \| Usage \| 用途 \|/);
assert.match(
  notes,
  /\| \[`codux-1\.8\.0-macos-aarch64\.dmg`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-1\.8\.0-macos-aarch64\.dmg\) \| Apple Silicon Mac stable release \| Apple Silicon Mac 正式版本 \|/,
);
assert.match(
  notes,
  /\| \[`codux-1\.8\.0-macos-x86_64\.dmg`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-1\.8\.0-macos-x86_64\.dmg\) \| Intel Mac stable release \| Intel Mac 正式版本 \|/,
);
assert.match(
  notes,
  /\| \[`codux-1\.8\.0-macos-aarch64-debug\.dmg`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-1\.8\.0-macos-aarch64-debug\.dmg\) \| Apple Silicon Mac debug build \| Apple Silicon Mac 测试版本 \|/,
);
assert.match(
  notes,
  /\| \[`codux-1\.8\.0-macos-x86_64-debug\.dmg`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-1\.8\.0-macos-x86_64-debug\.dmg\) \| Intel Mac debug build \| Intel Mac 测试版本 \|/,
);
assert.match(
  notes,
  /\| \[`codux-1\.8\.0-windows-x86_64-setup\.exe`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-1\.8\.0-windows-x86_64-setup\.exe\) \| Windows 64-bit installer \| Windows 64 位安装包 \|/,
);
assert.match(
  notes,
  /\| \[`codux-agent-1\.8\.0-macos-aarch64`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-agent-1\.8\.0-macos-aarch64\) \| Apple Silicon Mac headless agent \| Apple Silicon Mac 被控端 \|/,
);
assert.match(
  notes,
  /\| \[`codux-agent-1\.8\.0-macos-x86_64`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-agent-1\.8\.0-macos-x86_64\) \| Intel Mac headless agent \| Intel Mac 被控端 \|/,
);
assert.match(
  notes,
  /\| \[`codux-agent-1\.8\.0-linux-x86_64`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-agent-1\.8\.0-linux-x86_64\) \| Linux x86_64 headless agent \| Linux x86_64 被控端 \|/,
);
assert.match(
  notes,
  /\| \[`codux-agent-1\.8\.0-linux-aarch64`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-agent-1\.8\.0-linux-aarch64\) \| Linux ARM64 headless agent \| Linux ARM64 被控端 \|/,
);
assert.match(
  notes,
  /\| \[`codux-agent-1\.8\.0-windows-x86_64\.exe`\]\(https:\/\/github\.com\/duxweb\/codux\/releases\/download\/v1\.8\.0\/codux-agent-1\.8\.0-windows-x86_64\.exe\) \| Windows 64-bit headless agent \| Windows 64 位被控端 \|/,
);
assert.doesNotMatch(notes, /codux-\*/);
assert.doesNotMatch(notes, /latest\.json/);
assert.doesNotMatch(notes, /updater\.app\.tar\.gz/);

fs.rmSync(tempDir, { recursive: true, force: true });
console.log("prepare-release notes test passed");
