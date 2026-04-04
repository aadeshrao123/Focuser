#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod blocker;
mod commands;

use directories::ProjectDirs;
use focuser_core::{BlockEngine, Database};
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Shared application state accessible from all Tauri commands.
pub struct AppState {
    pub engine: Mutex<BlockEngine>,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Focuser starting");

    #[cfg(windows)]
    {
        if is_elevated() {
            info!("Running with admin privileges");
        } else {
            info!("Running without admin — hosts file blocking may not work");
        }
    }

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
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
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
            commands::bulk_import_websites,
            commands::add_exception,
            commands::remove_exception,
            commands::export_block_list,
            commands::clear_all_websites,
            commands::clear_all_apps,
        ])
        .setup(move |app| {
            // Spawn background blocking loop
            let blocker_state = Arc::clone(&state_for_blocker);
            std::thread::spawn(move || {
                blocker::run_blocking_loop(blocker_state);
            });

            // Spawn extension API server
            let api_state = Arc::clone(&state_for_blocker);
            std::thread::spawn(move || {
                api::run_api_server(api_state);
            });

            // System tray icon
            let show = MenuItemBuilder::with_id("show", "Open Focuser").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

            let icon = app.default_window_icon().cloned().unwrap();

            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Focuser — Blocking active")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        std::process::exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Poll for "show window" requests from the extension API
            let show_handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    #[allow(clippy::collapsible_if)]
                    if api::SHOW_WINDOW_REQUESTED
                        .swap(false, std::sync::atomic::Ordering::Relaxed)
                    {
                        if let Some(window) = show_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
            });

            // Close to tray instead of quitting
            let app_handle = app.handle().clone();
            let window = app.get_webview_window("main").unwrap();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(win) = app_handle.get_webview_window("main") {
                        let _ = win.hide();
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Focuser");
}

#[cfg(windows)]
fn is_elevated() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
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
