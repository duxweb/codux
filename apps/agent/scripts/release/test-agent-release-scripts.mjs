#!/usr/bin/env node
/* global console, process */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "codux-agent-release-test-"));
const target = "aarch64-apple-darwin";
const version = "1.9.1";

try {
  const binaryPath = path.join(tempDir, "target", target, "release", "codux-agent");
  fs.mkdirSync(path.dirname(binaryPath), { recursive: true });
  fs.writeFileSync(binaryPath, "#!/bin/sh\n", "utf8");
  fs.chmodSync(binaryPath, 0o755);

  run("node", [path.join(repoRoot, "apps/agent/scripts/release/package-agent.mjs")], {
    cwd: tempDir,
    env: {
      ...process.env,
      CARGO_BUILD_TARGET: target,
      RELEASE_BUILD_ID: "macos-aarch64",
      RELEASE_VERSION: version,
    },
  });

  const expected = path.join(tempDir, "release-artifacts", "macos-aarch64", "codux-macos-aarch64");
  const versioned = path.join(tempDir, "release-artifacts", "macos-aarch64", `codux-agent-${version}-macos-aarch64`);
  assert.ok(fs.existsSync(expected), "stable agent asset should exist");
  assert.ok(!fs.existsSync(versioned), "versioned agent asset should not be produced");
  fs.writeFileSync(path.join(tempDir, "release-artifacts", "codux-1.9.1-macos-aarch64.dmg"), "");
  fs.writeFileSync(path.join(tempDir, "release-artifacts", "latest.json"), "{}\n");

  const dryRun = run(
    "node",
    [path.join(repoRoot, "apps/agent/scripts/release/publish-agent-release.mjs"), "--dry-run"],
    {
      cwd: tempDir,
      env: {
        ...process.env,
        RELEASE_VERSION: version,
        RELEASE_TAG: `v${version}`,
        RELEASE_ARTIFACTS_DIR: path.join(tempDir, "release-artifacts"),
      },
      encoding: "utf8",
    },
  );
  assert.match(dryRun.stdout, /Prepared 1 agent assets/);
} finally {
  fs.rmSync(tempDir, { recursive: true, force: true });
}

console.log("agent release scripts test passed");

function run(command, args, options) {
  const result = spawnSync(command, args, {
    stdio: options?.encoding ? "pipe" : "inherit",
    env: process.env,
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
  return result;
}
