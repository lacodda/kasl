//! The mark, held against the level rule of the line.
//!
//! Three levels of the mark exist because three sizes of place exist: the
//! filled S tile at 27px and below (an outline collapses into noise there),
//! the plated M mark at 28-63px, the plated L mark with the metaphor at 64px
//! and up. The rule applies to every image *inside* the `.ico`, not to the
//! file as a whole - a 48px taskbar entry rendered from the S tile is a flat
//! coloured hexagon where the product's mark should be. Nothing held this
//! until the owner saw it on a sibling product, a month after it shipped.
//!
//! The checks read pixels, not file names: the exporter's list of which SVG
//! feeds which size is a script's decision, and the script is what was wrong.

use image::ImageFormat;
use std::path::Path;

/// One image inside an `.ico`, as the directory describes it.
struct Entry {
    size: u32,
    offset: usize,
    length: usize,
}

/// Read the directory of an `.ico`: reserved (2), type (2), count (2), then
/// 16 bytes per entry. A zero width means 256 - the field is one byte.
fn entries(ico: &[u8]) -> Vec<Entry> {
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    (0..count)
        .map(|index| {
            let at = 6 + index * 16;
            let entry = &ico[at..at + 16];
            let size = if entry[0] == 0 { 256 } else { entry[0] as u32 };
            let length = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
            assert!(ico.len() >= offset + length, "the {size}px image runs past the file");
            Entry { size, offset, length }
        })
        .collect()
}

fn asset(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Whether a PNG shows the filled S tile rather than the plated mark.
///
/// Counted over every visible pixel: the filled tile is mostly the brand
/// colour with a small dark code on it, the plated mark is mostly near-black
/// plate with the code and the metaphor in colour. The shares sit far apart
/// (about nine tenths bright against a quarter), so a one-half cut leaves
/// room on both sides - a single sample point does not, it can land on the
/// code either way.
fn is_filled_tile(png: &[u8], what: &str) -> bool {
    let decoded = image::load_from_memory_with_format(png, ImageFormat::Png).unwrap_or_else(|e| panic!("{what} is not a PNG: {e}"));
    let rgba = decoded.to_rgba8();
    let (visible, bright) = rgba.pixels().fold((0u32, 0u32), |(visible, bright), pixel| {
        let [r, g, b, a] = pixel.0;
        if a <= 40 {
            return (visible, bright);
        }
        let brightness = u32::from(r) + u32::from(g) + u32::from(b);
        (visible + 1, bright + u32::from(brightness > 180))
    });
    assert!(visible > 0, "{what} is fully transparent");
    bright * 2 > visible
}

/// The level rule of the line, held against the actual pixels of the `.ico`.
#[test]
fn every_size_carries_the_level_that_reads_at_it() {
    let ico = asset("icon.ico");
    let entries = entries(&ico);
    assert!(!entries.is_empty(), "icon.ico holds no images");
    for entry in &entries {
        let payload = &ico[entry.offset..entry.offset + entry.length];
        let what = format!("the {}px image", entry.size);
        let decoded = image::load_from_memory_with_format(payload, ImageFormat::Png).unwrap_or_else(|e| panic!("{what} is not a PNG: {e}"));
        assert_eq!(decoded.width(), entry.size, "{what} holds a {}px picture", decoded.width());

        let filled = is_filled_tile(payload, &what);
        if entry.size <= 27 {
            assert!(filled, "{what} is not the filled S tile; below 28px the outline collapses into noise");
        } else {
            assert!(
                !filled,
                "{what} is the filled S tile, not the plated mark - the level rule puts S at 27px and below"
            );
        }
    }
}

/// Largest first.
///
/// Windows picks by closest size and ignores order, but readers that take the
/// first entry verbatim exist - a sibling's titlebar was stretched from a 16px
/// entry for exactly this reason. Cheap to hold, expensive to notice.
#[test]
fn the_largest_image_comes_first() {
    let ico = asset("icon.ico");
    let sizes: Vec<u32> = entries(&ico).iter().map(|entry| entry.size).collect();
    let mut sorted = sizes.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(sizes, sorted, "the images are not ordered largest first");
}

/// 180px is L territory: a home-screen tile shows the mark, not a swatch.
#[test]
fn the_touch_icon_carries_the_plated_mark() {
    let png = asset("apple-touch-icon.png");
    assert!(
        !is_filled_tile(&png, "apple-touch-icon.png"),
        "apple-touch-icon.png is the filled S tile; at 180px the mark reads"
    );
    let docs_copy = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/public/apple-touch-icon.png");
    assert_eq!(
        std::fs::read(docs_copy).expect("docs/public/apple-touch-icon.png"),
        png,
        "the docs site serves a different touch icon"
    );
}

/// The one documented exception: a favicon is drawn into 16px of browser tab
/// whatever size the file is, so it stays the filled tile on purpose.
#[test]
fn the_favicon_stays_the_filled_tile() {
    let png = asset("favicon-32.png");
    assert!(
        is_filled_tile(&png, "favicon-32.png"),
        "favicon-32.png lost the filled tile; it is drawn at 16px in a tab"
    );
}
