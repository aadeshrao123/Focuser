//! Pulling an icon out of a Windows executable.
//!
//! Two steps. Turn whatever the rule says into a file on disk, then ask the
//! shell for that file's icon at the largest size it has and copy the pixels
//! out of GDI.
//!
//! Going through the system image list rather than `ExtractIconEx` is what gets
//! the 256px artwork modern programs ship; `ExtractIconEx` hands back 32px.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAGS_AND_ATTRIBUTES, WIN32_FIND_DATAW};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
    STGM_READ,
};
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_SYSICONINDEX, SHGetFileInfoW, SHGetImageList};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::{Interface as _, PCWSTR};

use super::Icon;

/// Largest first — a program that ships 256px artwork should not be represented
/// by its 32px fallback.
const SIZES: [u32; 3] = [
    windows::Win32::UI::Shell::SHIL_JUMBO,
    windows::Win32::UI::Shell::SHIL_EXTRALARGE,
    windows::Win32::UI::Shell::SHIL_LARGE,
];

/// The system image list is per-process shared state, and two threads reading
/// it at once intermittently come back empty-handed. Commands are dispatched on
/// a thread pool, so that is reachable simply by opening the Applications page
/// while something else polls. Icons are read once per page load, so holding a
/// lock across the call costs nothing worth measuring.
static SHELL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Carries the Start Menu index across a batch of lookups.
#[derive(Default)]
pub struct Loader {
    /// Executable name (lower case) → full path, from Start Menu shortcuts.
    ///
    /// Built at most once, and only when something is not found the cheap way,
    /// because reading every shortcut means a COM round trip apiece.
    shortcuts: OnceLock<HashMap<String, PathBuf>>,
}

impl Loader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&self, target: &str) -> Option<Icon> {
        // A poisoned lock here means some other caller panicked mid-icon. That
        // is no reason to leave the app without icons for the rest of the
        // session. Held across resolution too: reading a shortcut is COM work
        // on the apartment initialised just below.
        let _guard = SHELL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // The shell may load a handler to produce the icon, and those expect an
        // initialised apartment. Already-initialised is not an error here.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        let path = self.resolve(target)?;
        let index = system_icon_index(&path)?;
        SIZES.iter().find_map(|&size| icon_at(size, index))
    }

    /// Work out which file a rule refers to.
    ///
    /// A rule holds a full path or a bare executable name. For a bare name
    /// there are three places worth asking, cheapest first: the registry's App
    /// Paths, where installers *may* record where they put things; `PATH`; and
    /// the Start Menu, where practically everything with a window puts a
    /// shortcut. The third matters more than it sounds — Steam, for one,
    /// registers no App Path and is not on `PATH`, so without it a rule for
    /// `steam.exe` finds nothing at all.
    fn resolve(&self, target: &str) -> Option<PathBuf> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }

        let direct = Path::new(target);
        if direct.is_absolute() || target.contains(['/', '\\']) {
            return direct.is_file().then(|| direct.to_path_buf());
        }

        // Only executables have icons worth reading; a window-title rule is a
        // phrase, not a file, and must not send us hunting the filesystem.
        if !target.to_ascii_lowercase().ends_with(".exe") {
            return None;
        }

        app_paths_entry(target)
            .or_else(|| search_path(target))
            .or_else(|| self.start_menu_target(target))
    }

    fn start_menu_target(&self, name: &str) -> Option<PathBuf> {
        self.shortcuts
            .get_or_init(read_start_menu)
            .get(&name.to_ascii_lowercase())
            .cloned()
    }
}

/// `HKLM\…\App Paths\<name>`, then the per-user hive. The default value is the
/// full path to the executable.
fn app_paths_entry(name: &str) -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let subkey = format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{name}");

    [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
        .into_iter()
        .find_map(|hive| {
            let value: String = RegKey::predef(hive)
                .open_subkey(&subkey)
                .ok()?
                .get_value("")
                .ok()?;
            let path = PathBuf::from(value.trim_matches('"'));
            path.is_file().then_some(path)
        })
}

fn search_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(';')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| candidate.is_file())
}

/// What a shortcut says about the program it launches.
struct Shortcut {
    /// The `.lnk`'s own name, minus the extension. Shortcuts are named after
    /// the application, which is the only clue left when the target is not.
    stem: String,
    /// The executable the shortcut runs.
    target: Option<PathBuf>,
    /// The icon the shell draws for it, when the shortcut names one.
    icon: Option<PathBuf>,
}

/// Where to find the icon for an executable name, from the Start Menu.
///
/// Two passes, because two different things can identify a program.
///
/// Most shortcuts point straight at the program, so the target's file name is
/// the reliable key — `Steam.lnk` runs `steam.exe`. But a whole class of
/// installers (Squirrel, which is most Electron apps) points the shortcut at a
/// shared updater and names the real program in the arguments: Discord's
/// shortcut runs `Update.exe --processStart Discord.exe`. Keying on the target
/// there would file Discord under `update.exe`.
///
/// So the second pass keys on the shortcut's own name instead, and takes the
/// icon the shortcut declares — which is what the shell itself draws, and for
/// Discord is the `app.ico` sitting beside the updater. Second pass, so a real
/// executable always wins over a guess from a file name.
fn read_start_menu() -> HashMap<String, PathBuf> {
    icon_sources(&start_menu_roots())
}

fn icon_sources(roots: &[PathBuf]) -> HashMap<String, PathBuf> {
    let shortcuts: Vec<Shortcut> = roots
        .iter()
        .flat_map(|root| shortcuts_under(root))
        .filter_map(|path| read_shortcut(&path))
        .collect();

    let mut sources = HashMap::new();

    for shortcut in &shortcuts {
        let Some(target) = shortcut.target.as_ref() else {
            continue;
        };
        if target
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("exe"))
        {
            continue;
        }
        if let Some(name) = target.file_name() {
            sources
                .entry(name.to_string_lossy().to_ascii_lowercase())
                .or_insert_with(|| target.clone());
        }
    }

    for shortcut in &shortcuts {
        let Some(source) = shortcut.icon.clone().or_else(|| shortcut.target.clone()) else {
            continue;
        };
        sources
            .entry(format!("{}.exe", shortcut.stem.to_ascii_lowercase()))
            .or_insert(source);
    }

    sources
}

fn start_menu_roots() -> Vec<PathBuf> {
    const SUFFIX: &str = "Microsoft/Windows/Start Menu/Programs";

    ["ProgramData", "APPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|base| Path::new(&base).join(SUFFIX))
        .filter(|path| path.is_dir())
        .collect()
}

/// `.lnk` files under a Start Menu tree. Depth-limited because a stray
/// directory loop should not turn an icon lookup into a filesystem crawl.
fn shortcuts_under(root: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: u32 = 4;

    let mut found = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0u32)];

    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    pending.push((path, depth + 1));
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
            {
                found.push(path);
            }
        }
    }

    found
}

/// Read what a `.lnk` records.
///
/// `GetPath` returns the stored target; it does not go looking for one that has
/// moved, which is what we want — a stale shortcut should fail fast rather than
/// wake the link tracking service.
fn read_shortcut(shortcut: &Path) -> Option<Shortcut> {
    let stem = shortcut.file_stem()?.to_string_lossy().into_owned();

    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;

    let file: IPersistFile = link.cast().ok()?;
    let wide_path = wide(&shortcut.to_string_lossy());
    unsafe { file.Load(PCWSTR(wide_path.as_ptr()), STGM_READ) }.ok()?;

    let mut buffer = [0u16; 1024];
    let target = unsafe { link.GetPath(&mut buffer, &mut WIN32_FIND_DATAW::default(), 0) }
        .ok()
        .and_then(|()| existing_path(&buffer));

    let mut index = 0i32;
    let icon = unsafe { link.GetIconLocation(&mut buffer, &mut index) }
        .ok()
        // A non-zero index means "the Nth icon inside this file", which the
        // shell image list cannot address. Those fall back to the target.
        .filter(|()| index == 0)
        .and_then(|()| existing_path(&buffer));

    Some(Shortcut { stem, target, icon })
}

/// A NUL-terminated wide buffer as a path, if it names something on disk.
fn existing_path(buffer: &[u16]) -> Option<PathBuf> {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    if end == 0 {
        return None;
    }

    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..end]));
    path.is_file().then_some(path)
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// This file's index in the system image list — the same index across every
/// size, which is what lets us ask for the biggest one available.
fn system_icon_index(path: &Path) -> Option<i32> {
    let wide_path = wide(&path.to_string_lossy());
    let mut info = SHFILEINFOW::default();

    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide_path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX,
        )
    };

    (ok != 0).then_some(info.iIcon)
}

fn icon_at(size: u32, index: i32) -> Option<Icon> {
    let list: IImageList = unsafe { SHGetImageList(size as i32) }.ok()?;
    let handle = unsafe { list.GetIcon(index, ILD_TRANSPARENT.0) }.ok()?;

    let icon = read_pixels(handle);
    unsafe {
        let _ = DestroyIcon(handle);
    };
    icon
}

/// Copy an `HICON`'s pixels into an owned RGBA buffer.
fn read_pixels(handle: HICON) -> Option<Icon> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(handle, &mut info) }.ok()?;

    let colour = Bitmap(info.hbmColor);
    let mask = Bitmap(info.hbmMask);

    let (width, height) = dimensions(colour.0)?;
    let mut rgba = read_bgra(colour.0, width, height)?;

    // Swap BGRA to RGBA. GDI hands back little-endian 0xAARRGGBB, which is
    // B, G, R, A in memory order.
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    // Icons predating 32-bit colour carry no alpha channel at all; their
    // transparency lives in a separate 1-bit mask. Left as-is they decode as a
    // fully transparent square and get trimmed away to nothing.
    if rgba.chunks_exact(4).all(|px| px[3] == 0) {
        apply_mask(&mut rgba, mask.0, width, height)?;
    }

    Icon::new(width, height, rgba)
}

fn dimensions(bitmap: HBITMAP) -> Option<(u32, u32)> {
    let mut header = BITMAP::default();
    let written = unsafe {
        GetObjectW(
            bitmap,
            std::mem::size_of::<BITMAP>() as i32,
            Some(std::ptr::addr_of_mut!(header).cast()),
        )
    };

    if written == 0 || header.bmWidth <= 0 || header.bmHeight <= 0 {
        return None;
    }
    Some((header.bmWidth as u32, header.bmHeight as u32))
}

/// Ask GDI for the bitmap as 32bpp, top row first.
fn read_bgra(bitmap: HBITMAP, width: u32, height: u32) -> Option<Vec<u8>> {
    let dc = Dc(unsafe { CreateCompatibleDC(HDC::default()) });
    if dc.0.is_invalid() {
        return None;
    }

    let mut spec = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // Negative means top-down, matching how we index rows.
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
    let rows = unsafe {
        GetDIBits(
            dc.0,
            bitmap,
            0,
            height,
            Some(buffer.as_mut_ptr().cast()),
            &mut spec,
            DIB_RGB_COLORS,
        )
    };

    (rows != 0).then_some(buffer)
}

/// Fill in alpha from the 1-bit mask. Read back as 32bpp the mask comes through
/// as black where the icon shows and white where the background does.
fn apply_mask(rgba: &mut [u8], mask: HBITMAP, width: u32, height: u32) -> Option<()> {
    let bits = read_bgra(mask, width, height)?;

    for (px, m) in rgba.chunks_exact_mut(4).zip(bits.chunks_exact(4)) {
        px[3] = if m[0] > 127 { 0 } else { 255 };
    }
    Some(())
}

/// GDI handles are not freed for us, and an icon is fetched per row.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = DeleteObject(self.0);
            };
        }
    }
}

struct Dc(HDC);

impl Drop for Dc {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = DeleteDC(self.0);
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::BOOL;

    /// Write a real `.lnk`, so the reader is exercised rather than a stand-in.
    fn write_shortcut(path: &Path, target: &Path, icon: Option<(&Path, i32)>) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            link.SetPath(PCWSTR(wide(&target.to_string_lossy()).as_ptr()))
                .unwrap();

            if let Some((icon, index)) = icon {
                link.SetIconLocation(PCWSTR(wide(&icon.to_string_lossy()).as_ptr()), index)
                    .unwrap();
            }

            let file: IPersistFile = link.cast().unwrap();
            file.Save(PCWSTR(wide(&path.to_string_lossy()).as_ptr()), BOOL(1))
                .unwrap();
        }
    }

    /// Something that exists on disk and can stand in for a program.
    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn a_window_title_is_not_hunted_for_on_disk() {
        let loader = Loader::new();

        assert!(loader.resolve("Solitaire").is_none());
        assert!(loader.resolve("").is_none());
        assert!(loader.resolve("   ").is_none());
    }

    #[test]
    fn a_path_that_does_not_exist_resolves_to_nothing() {
        assert!(Loader::new().resolve("C:\\nope\\missing.exe").is_none());
    }

    #[test]
    fn a_name_on_the_path_resolves() {
        // Present on every Windows install, and always on PATH.
        assert!(Loader::new().resolve("notepad.exe").is_some());
    }

    #[test]
    fn a_shortcut_pointing_straight_at_a_program_is_keyed_by_that_program() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(&dir.path().join("Steam"), "steam.exe");
        write_shortcut(&dir.path().join("Steam.lnk"), &target, None);

        let sources = icon_sources(&[dir.path().to_path_buf()]);

        assert_eq!(sources.get("steam.exe"), Some(&target));
    }

    // Squirrel installers — most Electron apps — point the shortcut at a shared
    // updater and name the program in the arguments. Keyed on the target alone,
    // Discord would be filed under `update.exe` and never found.
    #[test]
    fn a_shortcut_through_an_updater_is_keyed_by_its_own_name() {
        let dir = tempfile::tempdir().unwrap();
        let updater = touch(&dir.path().join("Discord"), "Update.exe");
        let icon = touch(&dir.path().join("Discord"), "app.ico");
        write_shortcut(&dir.path().join("Discord.lnk"), &updater, Some((&icon, 0)));

        let sources = icon_sources(&[dir.path().to_path_buf()]);

        assert_eq!(sources.get("discord.exe"), Some(&icon));
        // The updater is still reachable under its own name.
        assert_eq!(sources.get("update.exe"), Some(&updater));
    }

    #[test]
    fn a_real_program_outranks_a_guess_from_a_shortcut_name() {
        let dir = tempfile::tempdir().unwrap();
        let real = touch(&dir.path().join("Real"), "app.exe");
        let other = touch(&dir.path().join("Other"), "other.exe");

        write_shortcut(&dir.path().join("Real.lnk"), &real, None);
        // A different shortcut that happens to be named "app".
        write_shortcut(&dir.path().join("app.lnk"), &other, None);

        let sources = icon_sources(&[dir.path().to_path_buf()]);

        assert_eq!(sources.get("app.exe"), Some(&real));
    }

    #[test]
    fn an_icon_index_other_than_zero_falls_back_to_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(&dir.path().join("Thing"), "thing.exe");
        let shared = touch(&dir.path().join("Thing"), "shell32.dll");
        // "the 15th icon inside this file" — not something the shell image
        // list can address for us.
        write_shortcut(&dir.path().join("Thing.lnk"), &target, Some((&shared, 15)));

        let sources = icon_sources(&[dir.path().to_path_buf()]);

        assert_eq!(sources.get("thing.exe"), Some(&target));
    }

    #[test]
    fn shortcuts_are_found_in_nested_folders() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(&dir.path().join("bin"), "nested.exe");
        let deep = dir.path().join("Vendor/Suite");
        std::fs::create_dir_all(&deep).unwrap();
        write_shortcut(&deep.join("Nested.lnk"), &target, None);

        let sources = icon_sources(&[dir.path().to_path_buf()]);

        assert_eq!(sources.get("nested.exe"), Some(&target));
    }

    #[test]
    fn a_shortcut_to_a_target_that_is_gone_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(&dir.path().join("bin"), "temporary.exe");
        write_shortcut(&dir.path().join("Temporary.lnk"), &target, None);
        std::fs::remove_file(&target).unwrap();

        assert!(icon_sources(&[dir.path().to_path_buf()]).is_empty());
    }

    #[test]
    fn a_real_executable_yields_a_real_icon() {
        let icon = Loader::new()
            .load("notepad.exe")
            .expect("notepad should have an icon");

        assert!(icon.width >= 16 && icon.height >= 16);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        // A blank square would mean the alpha handling silently gave up.
        assert!(icon.rgba.chunks_exact(4).any(|px| px[3] > 0));
    }
}
