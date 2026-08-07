// Downloads the kasl binary matching this package version from GitHub Releases.
const fs = require("fs");
const https = require("https");
const path = require("path");
const { spawnSync } = require("child_process");

const pkg = require("./package.json");
const REPO = "lacodda/kasl";
// The wrapper can be patched independently of the Rust binary: an explicit
// kasl.binary field pins the release tag, otherwise it follows the version.
const TAG = (pkg.kasl && pkg.kasl.binary) || `v${pkg.version}`;

const TARGETS = {
  "win32-x64": ["x86_64-pc-windows-msvc", "tar.gz"],
  "linux-x64": ["x86_64-unknown-linux-gnu", "tar.gz"],
  "darwin-arm64": ["aarch64-apple-darwin", "tar.gz"],
};

const key = `${process.platform}-${process.arch}`;
const entry = TARGETS[key];
if (!entry) {
  console.error(`kasl: no prebuilt binary for ${key} yet; install with: cargo install kasl-cli`);
  process.exit(1);
}
const [target, ext] = entry;
const name = `kasl-${TAG}-${target}`;
const url = `https://github.com/${REPO}/releases/download/${TAG}/${name}.${ext}`;
const exe = process.platform === "win32" ? "kasl.exe" : "kasl";
const archive = path.join(__dirname, `archive.${ext}`);

function download(url, file, redirects, done) {
  if (redirects > 5) return done(new Error("too many redirects"));
  https
    .get(url, { headers: { "user-agent": "kasl-npm" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        return download(res.headers.location, file, redirects + 1, done);
      }
      if (res.statusCode !== 200) {
        res.resume();
        return done(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      const out = fs.createWriteStream(file);
      res.pipe(out);
      out.on("finish", () => out.close(done));
      out.on("error", done);
    })
    .on("error", done);
}

console.log(`kasl: downloading ${url}`);
download(url, archive, 0, (err) => {
  if (err) {
    console.error(`kasl: download failed: ${err.message}`);
    process.exit(1);
  }
  // bsdtar on Windows and GNU tar on Unix both read tar.gz fine.
  const result = spawnSync("tar", ["-xzf", `archive.${ext}`], { cwd: __dirname, stdio: "inherit" });
  if (result.status !== 0) {
    console.error("kasl: cannot extract the archive");
    process.exit(1);
  }
  fs.renameSync(path.join(__dirname, name, exe), path.join(__dirname, exe));
  fs.rmSync(path.join(__dirname, name), { recursive: true, force: true });
  fs.rmSync(archive, { force: true });
  if (process.platform !== "win32") {
    fs.chmodSync(path.join(__dirname, exe), 0o755);
  }
  console.log(`kasl: installed kasl ${TAG}`);
});
