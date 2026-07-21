#!/usr/bin/env node
/* global process */

import fs from "node:fs";
import path from "node:path";

const [, , versionArg, armSha256, intelSha256, outputPath] = process.argv;
const version = (versionArg || "").replace(/^v/, "");

if (!version || !armSha256 || !intelSha256 || !outputPath) {
  throw new Error("usage: render-homebrew-cask.mjs <version> <arm-sha256> <intel-sha256> <output-path>");
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(
  outputPath,
  `cask "codux" do
  version "${version}"

  on_arm do
    sha256 "${armSha256}"
    url "https://github.com/duxweb/codux/releases/download/v#{version}/codux-macos-aarch64.dmg"
  end

  on_intel do
    sha256 "${intelSha256}"
    url "https://github.com/duxweb/codux/releases/download/v#{version}/codux-macos-x86_64.dmg"
  end

  name "Codux"
  desc "Native terminal workspace for AI coding tools"
  homepage "https://github.com/duxweb/codux"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :sonoma"

  app "Codux.app"

  zap trash: [
    "~/Library/Application Support/Codux",
    "~/Library/Caches/com.duxweb.codux",
    "~/Library/HTTPStorages/com.duxweb.codux",
    "~/Library/Preferences/com.duxweb.codux.plist",
    "~/Library/Saved Application State/com.duxweb.codux.savedState",
  ]
end
`,
  "utf8",
);
