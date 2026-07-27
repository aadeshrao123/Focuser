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

            // `max(1)` rather than a zero branch: colours are weighted by
            // alpha, so a fully transparent block already has r, g and b at
            // zero — dividing by 1 lands on the same transparent pixel.
            rgba.extend_from_slice(&[
                (r / a.max(1)) as u8,
                (g / a.max(1)) as u8,
                (b / a.max(1)) as u8,
                (a / n.max(1)) as u8,
            ]);
        }
    }

    Icon::new(width, height, rgba).unwrap_or_else(|| icon.clone())
}

/// Decode a PNG into RGBA8.
///
/// Linux icon themes are mostly PNG files on disk, and ICNS stores its larger
/// icons as embedded PNG. Palette, grayscale and 16-bit sources are normalised
/// to 8-bit channels first, so callers only ever deal with one layout.
pub fn decode_png(bytes: &[u8]) -> Option<Icon> {
    // A Cursor, not the slice: the decoder wants `BufRead + Seek`.
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;

    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buffer[..info.buffer_size()]
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer[..info.buffer_size()]
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buffer[..info.buffer_size()]
            .iter()
            .flat_map(|&g| [g, g, g, 255])
            .collect(),
        // `normalize_to_color8` expands a palette, so this is unreachable in
        // practice; refusing beats emitting garbled pixels if it ever is not.
        png::ColorType::Indexed => return None,
    };

    Icon::new(info.width, info.height, rgba)
}

/// The `[Desktop Entry]` group of a freedesktop desktop entry file.
///
/// Only the keys needed to match a rule to an icon. Per the spec the entries
/// are `Key=Value` with the whitespace around `=` ignored, `#` lines and blank
/// lines are comments, and keys are case-sensitive. Localised keys carry a
/// `[lang]` suffix and are skipped — an icon name is not translated.
///
/// Lives here rather than in the Linux module so its tests run on every
/// platform, not only the one that uses it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub no_display: bool,
}

pub fn parse_desktop_entry(text: &str) -> DesktopEntry {
    let mut entry = DesktopEntry::default();
    let mut inside = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            // Other groups (desktop actions, for instance) carry their own
            // Icon and Exec keys, which are not the application's.
            inside = group == "Desktop Entry";
            continue;
        }

        if !inside {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());

        match key {
            "Icon" => entry.icon = Some(value.to_string()),
            "Exec" => entry.exec = Some(value.to_string()),
            "TryExec" => entry.try_exec = Some(value.to_string()),
            "NoDisplay" => entry.no_display = value == "true",
            _ => {}
        }
    }

    entry
}

/// The program an `Exec=` line runs, without its arguments.
///
/// `Exec` is a command line, not a path: it carries arguments, `%f`-style field
/// codes, and may quote a path containing spaces. Only the first word is the
/// program.
pub fn exec_program(exec: &str) -> Option<&str> {
    let exec = exec.trim_start();

    let program = if let Some(rest) = exec.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        exec.split_whitespace().next()?
    };

    (!program.is_empty()).then_some(program)
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
    Loader::new().icon_for(target)
}

/// Icons for a whole list, in the order asked for.
///
/// Prefer this over calling [`icon_for`] in a loop. Linux has to search every
/// installed desktop entry to connect a program to an icon name, and a loop
/// would repeat that search once per rule; a loader does it once.
pub fn icons_for<'a>(targets: impl IntoIterator<Item = &'a str>) -> Vec<Option<String>> {
    let loader = Loader::new();
    targets
        .into_iter()
        .map(|target| loader.icon_for(target))
        .collect()
}

/// Holds whatever a platform wants to reuse across a batch of lookups.
pub struct Loader(platform::Loader);

impl Loader {
    pub fn new() -> Self {
        Self(platform::Loader::new())
    }

    pub fn icon_for(&self, target: &str) -> Option<String> {
        let icon = self.0.load(target)?;
        let trimmed = trim_transparent(&icon)?;
        to_data_uri(&downscale(&trimmed, MAX_EDGE))
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

/// What the platform back-ends aim for before trimming and scaling.
///
/// Larger than [`MAX_EDGE`] so there is detail to throw away rather than
/// upscale, and a size every convention actually stocks: an ICNS `ic08`, a
/// hicolor `128x128` directory.
///
/// Windows has no use for it — the shell image list is addressed by symbolic
/// size rather than by pixels.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) const TARGET_EDGE: u32 = 128;

#[cfg(windows)]
#[path = "appicon_windows.rs"]
mod platform;

#[cfg(target_os = "macos")]
#[path = "appicon_macos.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "appicon_linux.rs"]
mod platform;

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod platform {
    /// The BSDs largely follow the freedesktop conventions, so the Linux
    /// back-end would mostly work there — but "mostly" is not something to
    /// claim without a machine to check it on.
    pub struct Loader;

    impl Loader {
        pub fn new() -> Self {
            Self
        }

        pub fn load(&self, _target: &str) -> Option<super::Icon> {
            None
        }
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
    fn a_png_round_trips_through_encode_and_decode() {
        let original = spotted(16, 4, 4, 6, [12, 200, 90]);
        let uri = to_data_uri(&original).unwrap();
        let png = base64::engine::general_purpose::STANDARD
            .decode(uri.trim_start_matches("data:image/png;base64,"))
            .unwrap();

        assert_eq!(decode_png(&png).unwrap(), original);
    }

    #[test]
    fn a_png_without_an_alpha_channel_decodes_as_opaque() {
        // 1x1 opaque red, 8-bit RGB — no tRNS, no alpha channel.
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0]).unwrap();
        }

        let icon = decode_png(&png).unwrap();

        assert_eq!(icon.rgba, vec![255, 0, 0, 255]);
    }

    #[test]
    fn garbage_is_not_a_png() {
        assert!(decode_png(b"not a png at all").is_none());
        assert!(decode_png(&[]).is_none());
    }

    #[test]
    fn a_desktop_entry_yields_the_keys_that_matter() {
        let entry = parse_desktop_entry(
            "# a comment\n\
             \n\
             [Desktop Entry]\n\
             Type=Application\n\
             Name=Steam\n\
             Exec=/usr/bin/steam %U\n\
             TryExec=/usr/bin/steam\n\
             Icon=steam\n",
        );

        assert_eq!(entry.icon.as_deref(), Some("steam"));
        assert_eq!(entry.exec.as_deref(), Some("/usr/bin/steam %U"));
        assert_eq!(entry.try_exec.as_deref(), Some("/usr/bin/steam"));
        assert!(!entry.no_display);
    }

    #[test]
    fn the_spec_says_whitespace_around_the_equals_sign_is_ignored() {
        let entry = parse_desktop_entry("[Desktop Entry]\nIcon =  firefox  \n");

        assert_eq!(entry.icon.as_deref(), Some("firefox"));
    }

    // Desktop actions are separate groups carrying their own Icon and Exec.
    // Reading them as the application's is how you end up showing the icon for
    // "Open a New Private Window" instead of the browser.
    #[test]
    fn only_the_desktop_entry_group_is_read() {
        let entry = parse_desktop_entry(
            "[Desktop Entry]\n\
             Icon=firefox\n\
             Exec=firefox\n\
             \n\
             [Desktop Action new-private-window]\n\
             Icon=private-browsing\n\
             Exec=firefox --private-window\n",
        );

        assert_eq!(entry.icon.as_deref(), Some("firefox"));
        assert_eq!(entry.exec.as_deref(), Some("firefox"));
    }

    #[test]
    fn a_localised_key_does_not_override_the_plain_one() {
        let entry = parse_desktop_entry("[Desktop Entry]\nIcon=steam\nIcon[de]=dampf\n");

        assert_eq!(entry.icon.as_deref(), Some("steam"));
    }

    #[test]
    fn keys_are_case_sensitive_as_the_spec_requires() {
        assert!(
            parse_desktop_entry("[Desktop Entry]\nICON=steam\n")
                .icon
                .is_none()
        );
    }

    #[test]
    fn nodisplay_is_only_true_when_it_says_true() {
        assert!(parse_desktop_entry("[Desktop Entry]\nNoDisplay=true\n").no_display);
        assert!(!parse_desktop_entry("[Desktop Entry]\nNoDisplay=false\n").no_display);
        assert!(!parse_desktop_entry("[Desktop Entry]\n").no_display);
    }

    #[test]
    fn an_exec_line_reduces_to_its_program() {
        assert_eq!(exec_program("/usr/bin/steam %U"), Some("/usr/bin/steam"));
        assert_eq!(exec_program("firefox"), Some("firefox"));
        assert_eq!(exec_program("  spotify  --uri=%U"), Some("spotify"));
    }

    #[test]
    fn a_quoted_program_path_survives_its_spaces() {
        assert_eq!(
            exec_program("\"/opt/My App/bin/run\" --flag %f"),
            Some("/opt/My App/bin/run")
        );
    }

    #[test]
    fn an_empty_exec_line_names_no_program() {
        assert!(exec_program("").is_none());
        assert!(exec_program("   ").is_none());
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
        let raw = platform::Loader::new().load("notepad.exe").unwrap();
        let icon = trim_transparent(&raw).unwrap();
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
