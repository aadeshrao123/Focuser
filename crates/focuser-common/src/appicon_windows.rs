//! Pulling an icon out of a Windows executable.
//!
//! Two steps. Turn whatever the rule says into a file on disk, then ask the
//! shell for that file's icon at the largest size it has and copy the pixels
//! out of GDI.
//!
//! Going through the system image list rather than `ExtractIconEx` is what gets
//! the 256px artwork modern programs ship; `ExtractIconEx` hands back 32px.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_SYSICONINDEX, SHGetFileInfoW, SHGetImageList};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::PCWSTR;

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

/// Nothing to carry between lookups: the shell keeps its own index.
pub struct Loader;

impl Loader {
    pub fn new() -> Self {
        Self
    }

    pub fn load(&self, target: &str) -> Option<Icon> {
        load(target)
    }
}

fn load(target: &str) -> Option<Icon> {
    let path = resolve(target)?;

    // A poisoned lock here means some other caller panicked mid-icon. That is
    // no reason to leave the app without icons for the rest of the session.
    let _guard = SHELL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // The shell may load a handler to produce the icon, and those expect an
    // initialised apartment. Already-initialised is not an error here.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let index = system_icon_index(&path)?;
    SIZES.iter().find_map(|&size| icon_at(size, index))
}

/// Work out which file a rule refers to.
///
/// A rule holds either a full path or a bare executable name. For a bare name
/// the two places worth asking are the registry's App Paths — where installers
/// record where they put things — and `PATH`.
fn resolve(target: &str) -> Option<PathBuf> {
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

    app_paths_entry(target).or_else(|| search_path(target))
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

    #[test]
    fn a_window_title_is_not_hunted_for_on_disk() {
        assert!(resolve("Solitaire").is_none());
        assert!(resolve("").is_none());
        assert!(resolve("   ").is_none());
    }

    #[test]
    fn a_path_that_does_not_exist_resolves_to_nothing() {
        assert!(resolve("C:\\nope\\missing.exe").is_none());
    }

    #[test]
    fn a_name_on_the_path_resolves() {
        // Present on every Windows install, and always on PATH.
        assert!(resolve("notepad.exe").is_some());
    }

    #[test]
    fn a_real_executable_yields_a_real_icon() {
        let icon = load("notepad.exe").expect("notepad should have an icon");

        assert!(icon.width >= 16 && icon.height >= 16);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        // A blank square would mean the alpha handling silently gave up.
        assert!(icon.rgba.chunks_exact(4).any(|px| px[3] > 0));
    }
}
