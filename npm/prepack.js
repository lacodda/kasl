// Copies the root README into the package before it is packed.
//
// The npm page must show the same text as GitHub and crates.io, and npm only
// publishes what sits next to package.json. This used to be a step in the
// publish workflow, which meant a manual `npm publish` shipped a page with no
// README at all; `prepack` runs for every pack, CI or hand.
const fs = require("fs");
const path = require("path");

const source = path.join(__dirname, "..", "README.md");
const target = path.join(__dirname, "README.md");

fs.copyFileSync(source, target);
console.log("kasl: README.md copied from the repository root");
