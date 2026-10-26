//! Native shims — the handful of operations that genuinely cannot live in
//! `focuser_app::execute`.
//!
//! Everything else goes through `run_command`. What is left here either needs a
//! Tauri handle (file dialogs, the updater) or enumerates OS processes, neither
//! of which belongs in a portable command core. Each one is deliberately thin:
//! it moves bytes or paths around and makes no product decisions.

#[cfg(not(windows))]
use tauri_plugin_updater::UpdaterExt;

/// Pick an executable to block. Returns the name an `ExecutableName` rule
/// matches on, which is not always the file name — see `process::name_for_path`.
///
/// `(async)` is load-bearing. The plugin opens the dialog with
/// `run_on_main_thread`, so waiting for it *from* the main thread deadlocks and
/// the window hangs. That is what made Browse unusable on Linux.
#[tauri::command(async)]
pub fn pick_app_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let dialog = app.dialog().file().set_title("Select Application to Block");

    // Filters only where an executable actually carries an extension. rfd turns
    // every filter entry into `*.ext`, so on Linux even an "All files" filter
    // becomes `*.*` and hides every extensionless binary in /usr/bin.
    #[cfg(windows)]
    let dialog = dialog.add_filter("Programs", &["exe", "com", "bat", "cmd"]);
    #[cfg(target_os = "macos")]
    let dialog = dialog.add_filter("Applications", &["app"]);

    Ok(dialog
        .blocking_pick_file()
        .map(|picked| focuser_common::process::name_for_path(&picked.to_string())))
}

/// Pick a Focuser configuration file and return its contents.
/// `None` means the user cancelled. `(async)` for the same reason as above.
#[tauri::command(async)]
pub fn pick_import_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let Some(path) = app
        .dialog()
        .file()
        .set_title("Import Focuser Configuration")
        .add_filter("JSON", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    std::fs::read_to_string(path.to_string())
        .map(Some)
        .map_err(|e| format!("Read failed: {e}"))
}

/// Ask where to save an exported configuration and write it there.
/// Returns the chosen path, or `None` if the user cancelled.
///
/// The document itself comes from `Command::ExportConfiguration` — this only
/// decides where it lands.
#[tauri::command(async)]
pub fn save_configuration(app: tauri::AppHandle, json: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let default_name = format!(
        "focuser-config-{}.json",
        chrono::Local::now().format("%Y-%m-%d")
    );
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Export Focuser Configuration")
        .add_filter("JSON", &["json"])
        .set_file_name(&default_name)
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.to_string();
    std::fs::write(&path, &json).map_err(|e| format!("Write failed: {e}"))?;
    Ok(Some(path))
}

/// Browser processes currently running, used by `SystemSync::running_browsers`.
#[cfg(not(windows))]
pub fn detect_running_browsers() -> std::collections::HashSet<focuser_common::extension::BrowserType>
{
    std::collections::HashSet::new()
}

#[cfg(not(windows))]
#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let updater = app.updater_builder().build().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(serde_json::json!({
            "available": true,
            "version": update.version,
            "body": update.body.unwrap_or_default(),
        })),
        Ok(None) => Ok(serde_json::json!({ "available": false })),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(windows))]
#[tauri::command]
pub async fn do_update(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let updater = app.updater_builder().build().map_err(|e| e.to_string())?;
        let update = updater
            .check()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("No update available")?;
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Launch a specific browser at a URL — used to send someone to the extension
/// store page in the browser that actually needs it.
#[tauri::command]
pub fn open_browser_url(browser: String, url: String) -> Result<(), String> {
    launch_browser(&browser, &url, &[])
}

/// Also used by installation-only recovery to reopen the affected profile.
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

#[cfg(windows)]
async fn authority_update(
    state: &crate::AppState,
    request: focuser_ipc::UpdateRequest,
) -> Result<focuser_ipc::UpdateStatus, String> {
    let client = state.client.clone();
    match tokio::task::spawn_blocking(move || {
        client.request_blocking(focuser_ipc::RequestBody::Update(request))
    })
    .await
    .map_err(|_| "internal: Desktop update request worker failed".to_string())?
    .map_err(|e| e.to_string())?
    {
        focuser_ipc::ReplyBody::Update(s) => Ok(s),
        _ => Err("protocol: Unexpected update reply".into()),
    }
}
#[cfg(windows)]
#[tauri::command]
pub async fn check_for_update(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<serde_json::Value, String> {
    let status = authority_update(&state, focuser_ipc::UpdateRequest::Status {}).await?;
    let status = if status.transaction.is_none()
        || matches!(
            status.phase.as_str(),
            "complete" | "rolled_back" | "cancelled"
        ) {
        authority_update(&state, focuser_ipc::UpdateRequest::Check {}).await?
    } else {
        status
    };
    serde_json::to_value(status).map_err(|e| e.to_string())
}
#[cfg(windows)]
#[tauri::command]
pub async fn do_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    use focuser_ipc::UpdateRequest;
    let status = authority_update(&state, UpdateRequest::Status {}).await?;
    let result = match status.phase.as_str() {
        "available" => authority_update(&state, UpdateRequest::Stage {}).await?,
        "staged" | "deferred" => {
            authority_update(
                &state,
                UpdateRequest::Activate {
                    transaction: status.transaction.ok_or("update_missing")?,
                },
            )
            .await?
        }
        _ => return Err("update_busy: Check service update status".into()),
    };
    // Close only this interface normally. The SYSTEM broker owns the transaction.
    if result.phase == "authorized" {
        app.exit(0);
    }
    Ok(())
}
#[cfg(windows)]
#[tauri::command]
pub async fn update_status(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<serde_json::Value, String> {
    serde_json::to_value(authority_update(&state, focuser_ipc::UpdateRequest::Status {}).await?)
        .map_err(|e| e.to_string())
}
#[cfg(not(windows))]
#[tauri::command]
pub async fn update_status() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"phase":"legacy","code":"legacy"}))
}
#[cfg(windows)]
#[tauri::command]
pub async fn cancel_update(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    let status = authority_update(&state, focuser_ipc::UpdateRequest::Status {}).await?;
    authority_update(
        &state,
        focuser_ipc::UpdateRequest::Cancel {
            transaction: status.transaction.ok_or("update_missing")?,
        },
    )
    .await?;
    Ok(())
}
#[cfg(not(windows))]
#[tauri::command]
pub async fn cancel_update() -> Result<(), String> {
    Err("unsupported".into())
}
