// One-off: rasterize kasl brand SVGs into PNGs + a multi-size .ico.
// Run from the docs package so it resolves the local sharp install:
//   node export-assets.mjs
import sharp from "sharp";
import fs from "node:fs";
import path from "node:path";

const ASSETS = "C:/Projects/kasl/assets";

// The three levels of the mark. Which one a raster takes is decided by
// `levelFor` below, never by habit: `tests/brand_assets.rs` reads the pixels
// of what this script writes and fails the build when a size carries the
// wrong level.
const S = path.join(ASSETS, "logo-s.svg");
const M = path.join(ASSETS, "logo-m.svg");
const L = path.join(ASSETS, "logo.svg");
const BANNER = path.join(ASSETS, "banner.svg");

// Which level of the mark survives at which size - the line's rule, not a
// preference: S ≤27px, M 28–63px, L ≥64px. Below 28px the outline and the
// code collapse into noise, so the filled tile is all that reads; at 48px
// and up there is room for the mark the product is known by.
function levelFor(size) {
  if (size <= 27) return S;
  if (size <= 63) return M;
  return L;
}

// Largest first. Windows picks by *closest size* and ignores order (see
// "About Icons", Icon Display), but some readers take the first entry
// verbatim - a 16px first entry is a titlebar stretched from sixteen pixels.
const ICO_SIZES = [256, 128, 64, 48, 32, 24, 16];

async function png(src, size, out) {
  await sharp(src, { density: 384 }).resize(size, size).png().toFile(out);
}

// Minimal ICO container: header + directory entries + embedded PNG payloads.
function buildIco(pngBuffers, sizes) {
  const count = pngBuffers.length;
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(count, 4);

  const entries = Buffer.alloc(16 * count);
  let offset = 6 + 16 * count;
  pngBuffers.forEach((buf, i) => {
    const size = sizes[i];
    const e = 16 * i;
    entries.writeUInt8(size >= 256 ? 0 : size, e + 0); // width (0 means 256)
    entries.writeUInt8(size >= 256 ? 0 : size, e + 1); // height
    entries.writeUInt8(0, e + 2); // palette
    entries.writeUInt8(0, e + 3); // reserved
    entries.writeUInt16LE(1, e + 4); // color planes
    entries.writeUInt16LE(32, e + 6); // bits per pixel
    entries.writeUInt32LE(buf.length, e + 8);
    entries.writeUInt32LE(offset, e + 12);
    offset += buf.length;
  });

  return Buffer.concat([header, entries, ...pngBuffers]);
}

const icoParts = [];
for (const size of ICO_SIZES) {
  icoParts.push(await sharp(levelFor(size), { density: 384 }).resize(size, size).png().toBuffer());
}
fs.writeFileSync(path.join(ASSETS, "icon.ico"), buildIco(icoParts, ICO_SIZES));
console.log("wrote icon.ico");

// Favicon + docs logo.
// The one documented exception to `levelFor`: a favicon is drawn into 16px
// of browser tab whatever size the file is, and the outline does not survive
// that. The canon names it explicitly, so it is spelled out here rather than
// left looking like an oversight.
await png(S, 32, path.join(ASSETS, "favicon-32.png"));
await png(levelFor(180), 180, path.join(ASSETS, "apple-touch-icon.png"));
await png(levelFor(512), 512, path.join(ASSETS, "logo-512.png"));
// Toast appLogoOverride renders at 48 DIPs; 96px only covers 200% scaling,
// so the level is the one for 48px (M), not for the file's pixel count.
await png(levelFor(48), 96, path.join(ASSETS, "toast-96.png"));
console.log("wrote pngs");

// The docs site serves its own copy of the touch icon.
fs.copyFileSync(path.join(ASSETS, "apple-touch-icon.png"), "C:/Projects/kasl/docs/public/apple-touch-icon.png");

// GitHub social preview: 1280x640. Two adjustments to the banner: its plate
// spans the full 720px while the artwork only fills the left ~570px (trim the
// tail, or it lands off-centre), and the rounded plate over an identical
// background leaves a visible seam (drop it and keep the inner rows only).
const bannerWidth = 1600;
const bannerHeight = Math.round((bannerWidth * 170) / 720);
const inset = Math.round((bannerWidth * 6) / 720); // clears the plate's rounded edge
const banner = await sharp(BANNER, { density: 384 })
  .resize({ width: bannerWidth })
  .extract({ left: inset, top: inset, width: 1290 - inset, height: bannerHeight - 2 * inset })
  .png()
  .toBuffer();

await sharp({
  create: { width: 1280, height: 640, channels: 4, background: "#1B2126" },
})
  .composite([{ input: await sharp(banner).resize({ width: 880 }).png().toBuffer(), gravity: "centre" }])
  .png()
  .toFile(path.join(ASSETS, "social-preview.png"));
console.log("wrote social-preview.png");
