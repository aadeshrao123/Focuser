//! Pulling an icon out of a Linux desktop installation.
//!
//! Nothing on Linux ties an executable to an icon directly. The chain is
//! specified across two freedesktop documents:
//!
//! 1. Desktop Entry Specification — applications install a `.desktop` file in
//!    an `applications` directory under `$XDG_DATA_HOME` or `$XDG_DATA_DIRS`.
//!    Its `Exec`/`TryExec` keys name the program and its `Icon` key names the
//!    icon.
//! 2. Icon Theme Specification — the `Icon` value is a *name*, resolved through
//!    the current theme, its parents, then `hicolor`, then `/usr/share/pixmaps`.
//!    It may also be an absolute path, in which case it is used as-is.
//!
//! `freedesktop-icons` implements step 2 including theme inheritance and the
//! pixmaps fallback, so only step 1 is ours.
//!
//! Themes are permitted to ship PNG, XPM or SVG. PNG is preferred where both
//! exist; SVG is rasterised, since a good many applications now install only a
//! scalable icon. XPM is not handled — it is deprecated by the spec and rare.

use std::path::{Path, PathBuf};

// Through resvg's re-exports rather than as direct dependencies, so the
// rasteriser and its geometry types can never drift apart in version.
use resvg::{tiny_skia, usvg};

use super::{Icon, TARGET_EDGE, decode_png, exec_program, parse_desktop_entry};

/// Every installed desktop entry, parsed once.
///
/// Connecting a program to an icon name means searching all of them, and a
/// list of twenty rules would otherwise repeat that search twenty times.
pub struct Loader {
    entries: Vec<Entry>,
}

struct Entry {
    /// The desktop file's own name, minus `.desktop`.
    stem: String,
    icon: String,
    exec: Option<String>,
    try_exec: Option<String>,
}

impl Loader {
    pub fn new() -> Self {
        Self {
            entries: read_entries(&application_dirs()),
        }
    }

    pub fn load(&self, target: &str) -> Option<Icon> {
        let program = program_name(target)?;
        // An application with no desktop entry often still installs an icon
        // under its own name, so the program name is a reasonable last guess.
        let name = icon_name(&self.entries, &program).unwrap_or(program);
        let path = icon_path(&name)?;
        read(&path)
    }
}

/// The bare program name a rule refers to.
///
/// Rules hold either a name or a path; a window-title rule holds neither, and
/// its spaces are the giveaway.
fn program_name(target: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }

    let name = Path::new(target).file_name()?.to_str()?;

    (!name.is_empty() && !name.contains(' ')).then(|| name.to_string())
}

/// `applications` directories, in the precedence the spec gives them: the
/// user's `$XDG_DATA_HOME` first, then each `$XDG_DATA_DIRS` component.
fn application_dirs() -> Vec<PathBuf> {
    data_dirs()
        .into_iter()
        .map(|dir| dir.join("applications"))
        .collect()
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // "If $XDG_DATA_HOME is either not set or empty, a default equal to
    // $HOME/.local/share should be used."
    match std::env::var("XDG_DATA_HOME") {
        Ok(home) if !home.is_empty() => dirs.push(PathBuf::from(home)),
        _ => {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(Path::new(&home).join(".local/share"));
            }
        }
    }

    // "If $XDG_DATA_DIRS is either not set or empty, a value equal to
    // /usr/local/share/:/usr/share/ should be used."
    let shared = match std::env::var("XDG_DATA_DIRS") {
        Ok(value) if !value.is_empty() => value,
        _ => "/usr/local/share:/usr/share".to_string(),
    };
    dirs.extend(
        shared
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );

    dirs
}

/// Parse every `.desktop` file in the given directories, in precedence order.
fn read_entries(dirs: &[PathBuf]) -> Vec<Entry> {
    let mut entries = Vec::new();

    for path in dirs
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
    {
        if path.extension().is_none_or(|ext| ext != "desktop") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };

        let desktop = parse_desktop_entry(&text);
        // Hidden entries exist to register MIME types and the like; their icon
        // is not the application's.
        if desktop.no_display {
            continue;
        }
        let Some(icon) = desktop.icon.filter(|icon| !icon.is_empty()) else {
            continue;
        };

        entries.push(Entry {
            stem: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            icon,
            exec: desktop.exec,
            try_exec: desktop.try_exec,
        });
    }

    entries
}

/// Find the `Icon=` of the desktop entry that launches this program.
///
/// Ranked rather than taken first-found: an entry whose `TryExec` or `Exec`
/// actually names the program is a far better match than one that merely has a
/// similar filename. Ties go to the earlier entry, and the directories were
/// read in the precedence order the spec gives them.
fn icon_name(entries: &[Entry], program: &str) -> Option<String> {
    let names = |value: Option<&String>| {
        value
            .and_then(|v| exec_program(v))
            .and_then(|p| Path::new(p).file_name())
            .is_some_and(|name| name == program)
    };

    entries
        .iter()
        .filter_map(|entry| {
            let rank = if names(entry.try_exec.as_ref()) {
                0
            } else if names(entry.exec.as_ref()) {
                1
            } else if entry.stem == program {
                2
            } else {
                return None;
            };
            Some((rank, &entry.icon))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, icon)| icon.clone())
}

/// Resolve an `Icon=` value to a file.
fn icon_path(name: &str) -> Option<PathBuf> {
    // "The icon may be specified as an absolute path, in which case it is used
    // directly." A relative path with an extension is not a theme name either.
    let direct = Path::new(name);
    if direct.is_absolute() {
        return direct.is_file().then(|| direct.to_path_buf());
    }

    let mut lookup = freedesktop_icons::lookup(name)
        .with_size(TARGET_EDGE as u16)
        .with_cache();

    // Respect the user's chosen theme; the crate falls back to hicolor itself.
    let theme = freedesktop_icons::default_theme_gtk();
    if let Some(theme) = theme.as_deref() {
        lookup = lookup.with_theme(theme);
    }

    lookup.find()
}

fn read(path: &Path) -> Option<Icon> {
    let bytes = std::fs::read(path).ok()?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("svg") | Some("svgz") => render_svg(&bytes),
        _ => decode_png(&bytes),
    }
}

/// Rasterise an SVG icon at the target size.
///
/// Text rendering is deliberately not compiled in — icons are shapes, and font
/// support would pull a text-shaping stack into every Linux build.
fn render_svg(bytes: &[u8]) -> Option<Icon> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;

    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return None;
    }

    // Fit the target square without distorting a non-square icon.
    let scale = TARGET_EDGE as f32 / size.width().max(size.height());
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied alpha; the rest of the pipeline and PNG
    // both want it straight, and skipping this darkens every edge pixel.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|px| {
            let c = px.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();

    Icon::new(width, height, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_entry(dir: &Path, file: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(file), body).unwrap();
    }

    /// Read the fixture directories, then resolve, the way a Loader does.
    fn lookup(dirs: &[PathBuf], program: &str) -> Option<String> {
        icon_name(&read_entries(dirs), program)
    }

    #[test]
    fn a_window_title_is_not_mistaken_for_a_program() {
        assert!(program_name("Solitaire Deluxe").is_none());
        assert!(program_name("").is_none());
        assert!(program_name("   ").is_none());
    }

    #[test]
    fn a_path_reduces_to_its_program_name() {
        assert_eq!(program_name("/usr/bin/steam").as_deref(), Some("steam"));
        assert_eq!(program_name("steam").as_deref(), Some("steam"));
    }

    #[test]
    fn the_icon_comes_from_the_entry_that_launches_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("applications");
        write_entry(
            &apps,
            "org.example.Steam.desktop",
            "[Desktop Entry]\nType=Application\nExec=/usr/bin/steam %U\nIcon=steam\n",
        );

        assert_eq!(lookup(&[apps], "steam").as_deref(), Some("steam"));
    }

    #[test]
    fn a_reverse_dns_filename_is_no_obstacle() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("applications");
        write_entry(
            &apps,
            "com.visualstudio.code.desktop",
            "[Desktop Entry]\nExec=/usr/share/code/code %F\nIcon=vscode\n",
        );

        // The icon name matches neither the binary nor the file name.
        assert_eq!(lookup(&[apps], "code").as_deref(), Some("vscode"));
    }

    #[test]
    fn try_exec_outranks_a_merely_similar_filename() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("applications");
        write_entry(
            &apps,
            "foo.desktop",
            "[Desktop Entry]\nExec=something-else\nIcon=wrong-icon\n",
        );
        write_entry(
            &apps,
            "org.example.Real.desktop",
            "[Desktop Entry]\nTryExec=/usr/bin/foo\nExec=/usr/bin/foo\nIcon=right-icon\n",
        );

        assert_eq!(lookup(&[apps], "foo").as_deref(), Some("right-icon"));
    }

    #[test]
    fn hidden_entries_do_not_supply_the_icon() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("applications");
        write_entry(
            &apps,
            "hidden.desktop",
            "[Desktop Entry]\nExec=foo\nIcon=hidden-icon\nNoDisplay=true\n",
        );

        assert!(lookup(&[apps], "foo").is_none());
    }

    #[test]
    fn the_first_directory_in_precedence_order_wins() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user/applications");
        let system = dir.path().join("system/applications");
        write_entry(
            &user,
            "a.desktop",
            "[Desktop Entry]\nExec=foo\nIcon=user-icon\n",
        );
        write_entry(
            &system,
            "b.desktop",
            "[Desktop Entry]\nExec=foo\nIcon=system-icon\n",
        );

        assert_eq!(lookup(&[user, system], "foo").as_deref(), Some("user-icon"));
    }

    #[test]
    fn an_absolute_icon_path_is_used_directly() {
        let dir = tempfile::tempdir().unwrap();
        let icon = dir.path().join("custom.png");
        std::fs::write(&icon, b"").unwrap();

        assert_eq!(icon_path(icon.to_str().unwrap()), Some(icon.clone()));
        assert!(icon_path(&dir.path().join("missing.png").to_string_lossy()).is_none());
    }

    #[test]
    fn nothing_matches_when_no_entry_mentions_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("applications");
        write_entry(
            &apps,
            "other.desktop",
            "[Desktop Entry]\nExec=bar\nIcon=bar\n",
        );

        assert!(lookup(&[apps], "foo").is_none());
    }

    #[test]
    fn an_svg_icon_rasterises_to_the_target_size() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
            <rect width="32" height="32" fill="#ff0000"/>
        </svg>"##;

        let icon = render_svg(svg).expect("a plain rect should render");

        assert_eq!((icon.width, icon.height), (TARGET_EDGE, TARGET_EDGE));
        assert_eq!(&icon.rgba[..4], [255, 0, 0, 255]);
    }

    #[test]
    fn a_non_square_svg_keeps_its_aspect_ratio() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32" viewBox="0 0 64 32">
            <rect width="64" height="32" fill="#00ff00"/>
        </svg>"##;

        let icon = render_svg(svg).unwrap();

        assert_eq!((icon.width, icon.height), (TARGET_EDGE, TARGET_EDGE / 2));
    }

    #[test]
    fn a_transparent_svg_edge_is_not_darkened_by_premultiplication() {
        // A half-transparent white square. Read back premultiplied it would be
        // grey; demultiplied it stays white.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
            <rect width="16" height="16" fill="#ffffff" fill-opacity="0.5"/>
        </svg>"##;

        let icon = render_svg(svg).unwrap();

        assert_eq!(&icon.rgba[..3], [255, 255, 255]);
        assert!(
            (120..=136).contains(&icon.rgba[3]),
            "alpha was {}",
            icon.rgba[3]
        );
    }

    #[test]
    fn nonsense_is_not_an_svg() {
        assert!(render_svg(b"definitely not markup").is_none());
    }
}
