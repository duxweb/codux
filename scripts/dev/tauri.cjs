#!/usr/bin/env node

const cli = require("@tauri-apps/cli/main");

const args = process.argv.slice(2);
const isDevCommand = args[0] === "dev";
const hasConfig = args.some((arg) => arg === "--config" || arg.startsWith("--config="));

if (isDevCommand && !hasConfig) {
  args.push("--config", "src-tauri/tauri.dev.conf.json");
}

cli.run(args, "pnpm tauri").catch((error) => {
  cli.logError(error.message);
  process.exit(1);
});
