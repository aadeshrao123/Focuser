#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod blocker;
mod api;

use std::sync::{Arc, Mutex};
use directories::ProjectDirs;
use focuser_core::{BlockEngine, Database};
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Shared application state accessible from all Tauri commands.
pub struct AppState {
    pub engine: Mutex<BlockEngine>,
}

fn main() {
    // On Windows, re-launch as admin if not already elevated
    #[cfg(windows)]
    {
        if !is_elevated() {
            relaunch_elevated();
            return;
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Focuser starting (elevated)");

    // Initialize database
    let project_dirs = ProjectDirs::from("com", "focuser", "Focuser")
        .expect("Could not determine project directories");
    let data_dir = project_dirs.data_dir();
    std::fs::create_dir_all(data_dir).expect("Could not create data directory");

    let db_path = data_dir.join("focuser.db");
    info!(path = %db_path.display(), "Opening database");

    let db = Database::open(&db_path).expect("Could not open database");
    let engine = BlockEngine::new(db).expect("Could not initialize engine");

    let state = Arc::new(AppState {
        engine: Mutex::new(engine),
    });

    let state_for_blocker = Arc::clone(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::list_block_lists,
            commands::create_block_list,
            commands::update_block_list,
            commands::delete_block_list,
            commands::toggle_block_list,
            commands::add_website_rule,
            commands::remove_website_rule,
            commands::add_app_rule,
            commands::remove_app_rule,
            commands::check_domain,
            commands::get_stats,
            commands::apply_blocks,
            commands::remove_blocks,
        ])
        .setup(move |_app| {
            // Spawn background blocking loop
            let blocker_state = Arc::clone(&state_for_blocker);
            std::thread::spawn(move || {
                blocker::run_blocking_loop(blocker_state);
            });

            // Spawn extension API server (port 17549)
            let api_state = Arc::clone(&state_for_blocker);
            std::thread::spawn(move || {
                api::run_api_server(api_state);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Focuser");
}

/// Check if the current process is running with admin privileges.
#[cfg(windows)]
fn is_elevated() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );

        let _ = CloseHandle(token);
        result.is_ok() && elevation.TokenIsElevated != 0
    }
}

/// Re-launch this executable with admin privileges via ShellExecuteW "runas".
#[cfg(windows)]
fn relaunch_elevated() {
    use std::os::windows::ffi::OsStrExt;

    let exe = std::env::current_exe().expect("Cannot get current exe path");
    let exe_wide: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

    let verb: Vec<u16> = "runas\0".encode_utf16().collect();

    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(exe_wide.as_ptr()),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}
