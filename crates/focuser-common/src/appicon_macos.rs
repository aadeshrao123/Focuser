//! Pulling an icon out of a macOS application bundle.
//!
//! Deliberately no AppKit. `NSWorkspace.icon(forFile:)` would be the obvious
//! call, but it drags a GUI framework into a command core that also runs as a
//! daemon and a CLI. Everything needed is plain files: `Contents/Info.plist`
//! names the icon and `Contents/Resources` holds it.
//!
//! Apple's rule for `CFBundleIconFile`: "The filename you specify does not need
//! to include the extension… The system looks for the icon file in the main
//! resources directory of the bundle." When the extension is omitted the system
//! looks for `.icns`.
//!
//! Known gap: apps that ship their icon only inside a compiled asset catalog
//! (`Assets.car`, named by `CFBundleIconName`) are not covered. That format is
//! undocumented, and guessing at it is worse than falling back to the generated
//! tile. Bundles that also ship a loose `.icns` — still the common case — work.

use std::path::{Path, PathBuf};

use icns::{IconFamily, PixelFormat};

use super::{Icon, TARGET_EDGE};

/// Where applications live, in the order the system searches.
const ROOTS: [&str; 4] = [
    "/Applications",
    "/Applications/Utilities",
    "/System/Applications",
    "/System/Library/CoreServices",
];

/// The application directories, resolved once for a batch.
pub struct Loader {
    roots: Vec<PathBuf>,
}

impl Loader {
    pub fn new() -> Self {
        Self { roots: roots() }
    }

    pub fn load(&self, target: &str) -> Option<Icon> {
        let bundle = resolve(target, &self.roots)?;
        let icns = icon_file(&bundle)?;
        read_icns(&icns)
    }
}

fn roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ROOTS.iter().map(PathBuf::from).collect();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(Path::new(&home).join("Applications"));
    }
    roots
}

/// Work out which bundle a rule refers to.
///
/// A rule holds a bundle path, a bundle name, or the name of the executable
/// inside one. A path pointing *into* a bundle — which is what a process list
/// reports — is walked back up to the bundle itself.
fn resolve(target: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }

    if target.contains('/') {
        // `ancestors` starts at the path itself, so this covers both a bundle
        // path and a path to something inside one.
        return enclosing_bundle(Path::new(target));
    }

    // "Steam.app" and "Steam" should both find Steam.app. Matching is
    // case-insensitive because the filesystem usually is.
    let stem = target
        .strip_suffix(".app")
        .unwrap_or(target)
        .to_ascii_lowercase();

    roots
        .iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            is_bundle(path)
                && path
                    .file_stem()
                    .is_some_and(|name| name.to_string_lossy().to_ascii_lowercase() == stem)
        })
}

fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        && path.is_dir()
}

/// `/Applications/Foo.app/Contents/MacOS/foo` → `/Applications/Foo.app`.
fn enclosing_bundle(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| is_bundle(p))
        .map(Path::to_path_buf)
}

/// The bundle's `.icns`, named by `Info.plist` where possible.
fn icon_file(bundle: &Path) -> Option<PathBuf> {
    let resources = bundle.join("Contents/Resources");

    if let Some(named) = declared_icon_name(&bundle.join("Contents/Info.plist")) {
        // The extension is optional in the plist and defaults to .icns.
        let direct = resources.join(&named);
        if direct.is_file() {
            return Some(direct);
        }
        let with_ext = resources.join(format!("{named}.icns"));
        if with_ext.is_file() {
            return Some(with_ext);
        }
    }

    // No usable CFBundleIconFile. Bundles overwhelmingly ship exactly one
    // .icns in Resources, so take it rather than give up.
    sole_icns(&resources)
}

fn declared_icon_name(info_plist: &Path) -> Option<String> {
    // Info.plist is XML in some bundles and a binary plist in others; the
    // crate handles both, which is the reason to use it over reading the file.
    let value = plist::Value::from_file(info_plist).ok()?;
    let name = value
        .as_dictionary()?
        .get("CFBundleIconFile")?
        .as_string()?
        .trim();

    (!name.is_empty()).then(|| name.to_string())
}

fn sole_icns(resources: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(resources)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("icns"))
        })
        .collect();

    // Stable across runs, and "AppIcon.icns" beats "document.icns" on the
    // common convention that the app icon sorts first among few.
    candidates.sort();
    candidates.into_iter().find(|path| path.is_file())
}

/// Decode the best-sized icon in an ICNS family.
fn read_icns(path: &Path) -> Option<Icon> {
    let file = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let family = IconFamily::read(file).ok()?;

    // `available_icons` only lists types whose mask is present too, so every
    // candidate here can actually be decoded.
    let best = family
        .available_icons()
        .into_iter()
        .min_by_key(|icon_type| {
            let edge = icon_type.pixel_width().max(icon_type.pixel_height());
            // Smallest that still covers the target; failing that, the largest
            // available. Upscaling a 32px icon looks worse than downscaling.
            (edge < TARGET_EDGE, edge.abs_diff(TARGET_EDGE))
        })?;

    let image = family.get_icon_with_type(best).ok()?;
    let rgba = image.convert_to(PixelFormat::RGBA);

    Icon::new(rgba.width(), rgba.height(), rgba.data().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal bundle: `Contents/Info.plist` plus whatever resources are named.
    fn bundle(root: &Path, name: &str, plist_body: Option<&str>, resources: &[&str]) -> PathBuf {
        let bundle = root.join(format!("{name}.app"));
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(contents.join("Resources")).unwrap();
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();

        if let Some(body) = plist_body {
            std::fs::write(contents.join("Info.plist"), body).unwrap();
        }
        for resource in resources {
            std::fs::write(contents.join("Resources").join(resource), b"").unwrap();
        }
        bundle
    }

    fn plist_with_icon(name: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIconFile</key><string>{name}</string>
</dict></plist>"#
        )
    }

    #[test]
    fn a_bundle_is_found_by_name_with_or_without_the_extension() {
        let dir = tempfile::tempdir().unwrap();
        let steam = bundle(dir.path(), "Steam", None, &[]);
        let roots = vec![dir.path().to_path_buf()];

        assert_eq!(resolve("Steam", &roots).as_deref(), Some(steam.as_path()));
        assert_eq!(
            resolve("Steam.app", &roots).as_deref(),
            Some(steam.as_path())
        );
        // The filesystem is usually case-insensitive; matching should be too.
        assert_eq!(resolve("steam", &roots).as_deref(), Some(steam.as_path()));
        assert!(resolve("Nothing", &roots).is_none());
    }

    #[test]
    fn a_path_inside_a_bundle_walks_back_up_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let steam = bundle(dir.path(), "Steam", None, &[]);
        let inner = steam.join("Contents/MacOS/steam_osx");
        std::fs::write(&inner, b"").unwrap();

        let found = resolve(inner.to_str().unwrap(), &[]);

        assert_eq!(found.as_deref(), Some(steam.as_path()));
    }

    #[test]
    fn the_plist_names_the_icon_and_the_extension_is_optional() {
        let dir = tempfile::tempdir().unwrap();

        let with = bundle(
            dir.path(),
            "WithExt",
            Some(&plist_with_icon("Custom.icns")),
            &["Custom.icns"],
        );
        let without = bundle(
            dir.path(),
            "WithoutExt",
            Some(&plist_with_icon("Custom")),
            &["Custom.icns"],
        );

        assert_eq!(
            icon_file(&with),
            Some(with.join("Contents/Resources/Custom.icns"))
        );
        assert_eq!(
            icon_file(&without),
            Some(without.join("Contents/Resources/Custom.icns"))
        );
    }

    #[test]
    fn a_bundle_with_no_usable_plist_key_falls_back_to_the_icns_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let app = bundle(
            dir.path(),
            "NoKey",
            Some("not a plist at all"),
            &["AppIcon.icns"],
        );

        assert_eq!(
            icon_file(&app),
            Some(app.join("Contents/Resources/AppIcon.icns"))
        );
    }

    #[test]
    fn a_plist_pointing_at_a_missing_file_still_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let app = bundle(
            dir.path(),
            "Stale",
            Some(&plist_with_icon("Gone")),
            &["Actual.icns"],
        );

        assert_eq!(
            icon_file(&app),
            Some(app.join("Contents/Resources/Actual.icns"))
        );
    }

    #[test]
    fn a_bundle_with_no_icon_at_all_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let app = bundle(dir.path(), "Bare", None, &["readme.txt"]);

        assert!(icon_file(&app).is_none());
    }

    #[test]
    fn a_window_title_is_not_hunted_for_on_disk() {
        assert!(resolve("", &roots()).is_none());
        assert!(resolve("   ", &roots()).is_none());
    }

    /// Decode whatever this machine actually has.
    ///
    /// Deliberately not pinned to a named application. Which system apps still
    /// ship a loose `.icns` rather than an asset catalog changes between macOS
    /// releases, and a test asserting one does would be testing Apple's
    /// packaging rather than this code.
    #[test]
    fn a_real_installed_application_yields_a_real_icon() {
        let loader = Loader::new();

        let bundles: Vec<PathBuf> = loader
            .roots
            .iter()
            .filter_map(|root| std::fs::read_dir(root).ok())
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| is_bundle(path))
            .collect();

        let Some(icon) = bundles
            .iter()
            .find_map(|bundle| loader.load(&bundle.to_string_lossy()))
        else {
            eprintln!("no bundle among {} ships a loose .icns", bundles.len());
            return;
        };

        assert!(icon.width >= 16 && icon.height >= 16);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert!(icon.rgba.chunks_exact(4).any(|px| px[3] > 0));
    }
}
