//! Real application icons, read out of the executable on disk.
//!
//! An application rule is a name or a path, and a monogram tile stands in for
//! it everywhere else in the UI. On Windows the genuine icon is already on the
//! machine — inside the `.exe` — so there is no reason to settle for a letter,
//! and no reason to ask the network for one.
//!
//! Nothing here touches the network. That is the point: fetching an icon for
//! every entry would hand the whole block list to whoever served it.
//!
//! The pipeline is Windows-specific only in its first step. Trimming, scaling
//! and encoding are ordinary pixel work, kept platform-free so they can be
//! tested anywhere.

use base64::Engine as _;

/// A decoded icon: RGBA8, row-major, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Icons are drawn at ~36 CSS pixels. Anything past this is bytes nobody sees,
/// and a data URI is carried inline in every response.
const MAX_EDGE: u32 = 64;

/// Below this an alpha value is treated as fully transparent when trimming.
const ALPHA_FLOOR: u8 = 8;

impl Icon {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 || rgba.len() != (width as usize * height as usize * 4) {
            return None;
        }
        Some(Self {
            width,
            height,
            rgba,
        })
    }

    fn alpha_at(&self, x: u32, y: u32) -> u8 {
        self.rgba[((y * self.width + x) * 4 + 3) as usize]
    }

    fn pixel_at(&self, x: u32, y: u32) -> &[u8] {
        let i = ((y * self.width + x) * 4) as usize;
        &self.rgba[i..i + 4]
    }
}

/// Drop fully transparent rows and columns around the edges.
///
/// The shell hands back a fixed-size canvas — ask for a 256px icon and a
/// program that only ships a 32px one comes back as a 32px image adrift in a
/// transparent square. Without this they render as a speck.
///
/// `None` when the icon is entirely transparent, which is not worth showing.
pub fn trim_transparent(icon: &Icon) -> Option<Icon> {
    let opaque = |x: u32, y: u32| icon.alpha_at(x, y) > ALPHA_FLOOR;

    let mut left = icon.width;
    let mut right = 0;
    let mut top = icon.height;
    let mut bottom = 0;

    for y in 0..icon.height {
        for x in 0..icon.width {
            if !opaque(x, y) {
                continue;
            }
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
    }

    if left > right || top > bottom {
        return None;
    }
    if left == 0 && top == 0 && right == icon.width - 1 && bottom == icon.height - 1 {
        return Some(icon.clone());
    }

    let width = right - left + 1;
    let height = bottom - top + 1;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in top..=bottom {
        for x in left..=right {
            rgba.extend_from_slice(icon.pixel_at(x, y));
        }
    }

    Icon::new(width, height, rgba)
}

/// Box-filter downscale so the longest edge is at most `max_edge`.
///
/// Colour is averaged weighted by alpha. Averaging straight RGB instead pulls
/// in whatever sits under transparent pixels — usually black — and leaves a
/// dark halo around everything.
pub fn downscale(icon: &Icon, max_edge: u32) -> Icon {
    let longest = icon.width.max(icon.height);
    if longest <= max_edge {
        return icon.clone();
    }

    let width = (icon.width * max_edge / longest).max(1);
    let height = (icon.height * max_edge / longest).max(1);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        let y0 = y * icon.height / height;
        let y1 = (((y + 1) * icon.height).div_ceil(height))
            .min(icon.height)
            .max(y0 + 1);

        for x in 0..width {
            let x0 = x * icon.width / width;
            let x1 = (((x + 1) * icon.width).div_ceil(width))
                .min(icon.width)
                .max(x0 + 1);

            let (mut r, mut g, mut b, mut a) = (0u64, 0u64, 0u64, 0u64);
            let mut n = 0u64;

            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = icon.pixel_at(sx, sy);
                    let alpha = p[3] as u64;
                    r += p[0] as u64 * alpha;
                    g += p[1] as u64 * alpha;
                    b += p[2] as u64 * alpha;
                    a += alpha;
                    n += 1;
                }
            }

            if a == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                rgba.extend_from_slice(&[
                    (r / a) as u8,
                    (g / a) as u8,
                    (b / a) as u8,
                    (a / n.max(1)) as u8,
                ]);
            }
        }
    }

    Icon::new(width, height, rgba).unwrap_or_else(|| icon.clone())
}

/// Encode as a PNG `data:` URI, ready to drop straight into an `<img src>`.
pub fn to_data_uri(icon: &Icon) -> Option<String> {
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, icon.width, icon.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&icon.rgba).ok()?;
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
    Some(format!("data:image/png;base64,{encoded}"))
}

/// The icon for an application rule's value, as a data URI.
///
/// `target` is whatever the rule holds — `steam.exe`, a full path, or a window
/// title. `None` whenever there is nothing to show, which callers render as the
/// usual monogram tile.
pub fn icon_for(target: &str) -> Option<String> {
    let icon = platform::load(target)?;
    let trimmed = trim_transparent(&icon)?;
    to_data_uri(&downscale(&trimmed, MAX_EDGE))
}

#[cfg(windows)]
#[path = "appicon_windows.rs"]
mod platform;

#[cfg(not(windows))]
mod platform {
    /// macOS keeps icons in the bundle's resource fork and Linux in a themed
    /// icon directory. Both are worth doing; neither is this.
    pub fn load(_target: &str) -> Option<super::Icon> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `size`×`size` icon with an opaque `block`×`block` square at `(x, y)`.
    fn spotted(size: u32, x: u32, y: u32, block: u32, colour: [u8; 3]) -> Icon {
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for sy in y..y + block {
            for sx in x..x + block {
                let i = ((sy * size + sx) * 4) as usize;
                rgba[i..i + 3].copy_from_slice(&colour);
                rgba[i + 3] = 255;
            }
        }
        Icon::new(size, size, rgba).unwrap()
    }

    fn solid(size: u32, colour: [u8; 4]) -> Icon {
        Icon::new(size, size, colour.repeat((size * size) as usize)).unwrap()
    }

    #[test]
    fn an_icon_must_carry_exactly_as_many_pixels_as_it_claims() {
        assert!(Icon::new(2, 2, vec![0; 16]).is_some());
        assert!(Icon::new(2, 2, vec![0; 15]).is_none());
        assert!(Icon::new(0, 4, vec![]).is_none());
    }

    #[test]
    fn trimming_crops_away_the_transparent_canvas() {
        let icon = spotted(64, 16, 20, 8, [255, 0, 0]);

        let trimmed = trim_transparent(&icon).unwrap();

        assert_eq!((trimmed.width, trimmed.height), (8, 8));
        assert_eq!(trimmed.pixel_at(0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn trimming_leaves_a_full_bleed_icon_untouched() {
        let icon = solid(8, [10, 20, 30, 255]);

        assert_eq!(trim_transparent(&icon).unwrap(), icon);
    }

    #[test]
    fn an_entirely_transparent_icon_is_not_worth_showing() {
        assert!(trim_transparent(&solid(16, [0, 0, 0, 0])).is_none());
    }

    #[test]
    fn nearly_transparent_edges_do_not_count_as_content() {
        let mut icon = spotted(16, 4, 4, 4, [0, 200, 0]);
        // A stray anti-aliasing remnant in the corner.
        icon.rgba[3] = ALPHA_FLOOR;

        let trimmed = trim_transparent(&icon).unwrap();

        assert_eq!((trimmed.width, trimmed.height), (4, 4));
    }

    #[test]
    fn downscaling_caps_the_longest_edge_and_keeps_the_aspect_ratio() {
        let icon = Icon::new(256, 128, vec![255; 256 * 128 * 4]).unwrap();

        let small = downscale(&icon, 64);

        assert_eq!((small.width, small.height), (64, 32));
    }

    #[test]
    fn downscaling_leaves_an_already_small_icon_alone() {
        let icon = solid(32, [1, 2, 3, 255]);

        assert_eq!(downscale(&icon, 64), icon);
    }

    #[test]
    fn downscaling_averages_colour_rather_than_dropping_pixels() {
        // Two opaque halves: pure red on the left, pure blue on the right.
        let mut rgba = Vec::new();
        for _ in 0..128 {
            for x in 0..128 {
                rgba.extend_from_slice(if x < 64 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        let icon = Icon::new(128, 128, rgba).unwrap();

        let small = downscale(&icon, 2);

        assert_eq!(small.pixel_at(0, 0), [255, 0, 0, 255]);
        assert_eq!(small.pixel_at(1, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn transparent_pixels_do_not_darken_their_neighbours() {
        // Half opaque white, half fully transparent black. Averaging the raw
        // RGB would give mid-grey; weighting by alpha keeps it white.
        let mut rgba = Vec::new();
        for _ in 0..8 {
            for x in 0..8 {
                rgba.extend_from_slice(if x < 4 {
                    &[255, 255, 255, 255]
                } else {
                    &[0, 0, 0, 0]
                });
            }
        }
        let icon = Icon::new(8, 8, rgba).unwrap();

        let small = downscale(&icon, 1);
        let p = small.pixel_at(0, 0);

        assert_eq!(&p[..3], [255, 255, 255]);
        // Still half-covered, so half-transparent.
        assert!((120..=136).contains(&p[3]), "alpha was {}", p[3]);
    }

    #[test]
    fn a_data_uri_is_a_png_the_browser_will_accept() {
        let uri = to_data_uri(&solid(4, [9, 9, 9, 255])).unwrap();

        assert!(uri.starts_with("data:image/png;base64,"));

        let payload = uri.trim_start_matches("data:image/png;base64,");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn nothing_resolves_for_a_target_that_is_not_a_program() {
        assert!(icon_for("").is_none());
        assert!(icon_for("Solitaire").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_real_program_produces_a_small_png_data_uri() {
        let uri = icon_for("notepad.exe").expect("notepad should have an icon");
        let icon = trim_transparent(&platform::load("notepad.exe").unwrap()).unwrap();
        let scaled = downscale(&icon, MAX_EDGE);

        eprintln!(
            "notepad: {}x{} trimmed → {}x{} scaled → {} byte uri",
            icon.width,
            icon.height,
            scaled.width,
            scaled.height,
            uri.len()
        );

        assert!(uri.starts_with("data:image/png;base64,"));
        assert!(scaled.width <= MAX_EDGE && scaled.height <= MAX_EDGE);
        // Small enough that a list of them can travel inline in one response.
        assert!(uri.len() < 16_000, "data uri was {} bytes", uri.len());
    }
}
