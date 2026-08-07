#!/usr/bin/env node
// Thin launcher: forwards everything to the kasl binary, downloading it on
// first run when the postinstall script was skipped (npm v12 default).
const fs = require("fs");
const { spawnSync } = require("child_process");
const { install, exePath } = require("./download");

function exec() {
  const result = spawnSync(exePath, process.argv.slice(2), { stdio: "inherit" });
  process.exit(result.status === null ? 1 : result.status);
}

if (fs.existsSync(exePath)) {
  exec();
} else {
  console.error("kasl: binary not present yet - downloading it now");
  install(exec);
}
