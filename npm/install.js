// Postinstall entry: pre-downloads the binary where lifecycle scripts are
// allowed. On npm v12+ this script is skipped by default and run.js downloads
// the binary on first launch instead.
require("./download").install(() => {});
