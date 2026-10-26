pub(crate) fn launch_browser(browser: &str, url: &str, args: &[String]) -> Result<(), String> {
    let exe_path = resolve_browser_exe(browser);
    std::process::Command::new(&exe_path)
        .args(args)
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open {browser} at {exe_path}: {e}"))?;
    Ok(())
}

/// Resolve a browser short name to its full executable path by querying the
/// Windows Registry `App Paths` key — the standard mechanism Windows uses to
/// locate installed applications regardless of install location.
#[cfg(windows)]
fn resolve_browser_exe(browser: &str) -> String {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RegCloseKey, RegOpenKeyExW,
        RegQueryValueExW,
    };
    use windows::core::PCWSTR;

    let exe_name = match browser {
        "chrome" => "chrome.exe",
        "firefox" => "firefox.exe",
        "msedge" => "msedge.exe",
        "brave" => "brave.exe",
        "opera" => "opera.exe",
        other => return other.to_string(),
    };

    let sub_key = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{}",
        exe_name
    );
    let wide_key: Vec<u16> = sub_key.encode_utf16().chain(std::iter::once(0)).collect();

    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let mut hkey = HKEY::default();
        let status =
            unsafe { RegOpenKeyExW(root, PCWSTR(wide_key.as_ptr()), 0, KEY_READ, &mut hkey) };
        if status.is_err() {
            continue;
        }

        let mut buf = vec![0u8; 1024];
        let mut buf_len = buf.len() as u32;

        let status = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR::null(),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut buf_len),
            )
        };

        unsafe {
            let _ = RegCloseKey(hkey);
        }

        if status.is_ok() && buf_len > 2 {
            let wide_buf: Vec<u16> = buf[..buf_len as usize]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| u16::from_le_bytes(c))
                .collect();
            let len = wide_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(wide_buf.len());
            let path = String::from_utf16_lossy(&wide_buf[..len]);
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                return path;
            }
        }
    }

    if let Some(path) = find_exe_in_user_dirs(exe_name) {
        return path;
    }

    exe_name.to_string()
}

#[cfg(windows)]
fn find_exe_in_user_dirs(exe_name: &str) -> Option<String> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let candidates: Vec<std::path::PathBuf> = match exe_name {
        "opera.exe" => vec![
            [&local, "Programs", "Opera", "opera.exe"].iter().collect(),
            [&local, "Programs", "Opera GX", "opera.exe"]
                .iter()
                .collect(),
        ],
        "brave.exe" => vec![
            [
                &local,
                "BraveSoftware",
                "Brave-Browser",
                "Application",
                "brave.exe",
            ]
            .iter()
            .collect(),
        ],
        "chrome.exe" => vec![
            [&local, "Google", "Chrome", "Application", "chrome.exe"]
                .iter()
                .collect(),
        ],
        _ => vec![],
    };
    candidates
        .into_iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(not(windows))]
fn resolve_browser_exe(browser: &str) -> String {
    browser.to_string()
}
