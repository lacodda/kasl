#!/usr/bin/env node
// Thin launcher: forwards everything to the kasl binary, downloading it on
// first run when the postinstall script was skipped (npm v12 default).
const fs = require("fs");
const os = require("os");
const { spawnSync } = require("child_process");
const { install, exePath } = require("./download");

// The keystrokes a terminal sends to every process on it, this launcher
// included. Left to the default, the launcher died at once (exit 0xC000013A
// on Windows) and handed the prompt back while kasl - `watch --foreground`,
// a picker, a wizard - was still deciding what to do with the same keystroke.
// Listening keeps the launcher alive until the binary is done; the binary
// alone decides.
const KEYSTROKES =
  process.platform === "win32" ? ["SIGINT", "SIGBREAK"] : ["SIGINT", "SIGQUIT"];

function exec() {
  const stayPut = () => {};
  for (const signal of KEYSTROKES) process.on(signal, stayPut);

  const result = spawnSync(exePath, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    console.error(`kasl: cannot start ${exePath}: ${result.error.message}`);
    process.exit(1);
  }
  if (result.signal) {
    // Ended by a signal: end the same way, so the shell reports it as it
    // would for the binary run directly (130 after Ctrl+C), not as a plain 1.
    for (const signal of KEYSTROKES) process.removeListener(signal, stayPut);
    process.kill(process.pid, result.signal);
    process.exit(128 + (os.constants.signals[result.signal] || 0));
  }
  process.exit(result.status);
}

if (fs.existsSync(exePath)) {
  exec();
} else {
  console.error("kasl: binary not present yet - downloading it now");
  install(exec);
}
