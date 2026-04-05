#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod blocker;
mod commands;

use directories::ProjectDirs;
use focuser_core::{BlockEngine, Database};
use std::sync::{Arc, Mutex};
use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
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
        .plugin(tauri_plugin_dialog::init())
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
            commands::get_blocked_events,
            commands::apply_blocks,
            commands::remove_blocks,
            commands::bulk_import_websites,
            commands::add_exception,
            commands::remove_exception,
            commands::export_block_list,
            commands::clear_all_websites,
            commands::clear_all_apps,
            commands::pick_app_file,
            commands::update_schedule,
        ])
        .setup(move |app| {
            // Enable autostart by default on first run
            {
                use tauri_plugin_autostart::ManagerExt;
                let autostart = app.autolaunch();
                if !autostart.is_enabled().unwrap_or(false) {
                    let _ = autostart.enable();
                    info!("Autostart enabled by default");
                }
            }

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

            // Poll for "show window" and "install extension" requests
            let show_handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // Show window requests
                    if api::SHOW_WINDOW_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed)
                        && let Some(window) = show_handle.get_webview_window("main")
                    {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }

                    // Extension install prompt — show window + in-app modal
                    if api::EXTENSION_PROMPT_REQUESTED
                        .swap(false, std::sync::atomic::Ordering::Relaxed)
                    {
                        let browser_name =
                            api::take_killed_browser().unwrap_or_else(|| "your browser".into());

                        if let Some(window) = show_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();

                            // Inject themed in-app modal with retry
                            // The webview may not be ready immediately after show()
                            let js = build_extension_modal_js(&browser_name);
                            let win = window.clone();
                            std::thread::spawn(move || {
                                // Try multiple times with increasing delays
                                for delay_ms in [500, 1000, 1500] {
                                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                                    if win.eval(&js).is_ok() {
                                        break;
                                    }
                                }
                            });
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

/// Build JavaScript to inject a themed modal into the Focuser UI.
fn build_extension_modal_js(browser_name: &str) -> String {
    format!(
        r##"(function() {{
  // Remove existing modal if any
  var old = document.getElementById('focuser-ext-modal-overlay');
  if (old) old.remove();

  // Create overlay
  var overlay = document.createElement('div');
  overlay.id = 'focuser-ext-modal-overlay';
  overlay.style.cssText = 'position:fixed;top:0;left:0;width:100%;height:100%;background:rgba(0,0,0,0.6);backdrop-filter:blur(4px);z-index:99999;display:flex;align-items:center;justify-content:center;animation:focuserFadeIn 0.2s ease';

  // Create modal
  var modal = document.createElement('div');
  modal.style.cssText = 'background:#1e1e24;border:1px solid rgba(255,255,255,0.1);border-radius:12px;padding:32px;max-width:480px;width:90%;box-shadow:0 8px 32px rgba(0,0,0,0.6);font-family:Inter,-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;color:#f0f0f3;animation:focuserSlideIn 0.25s ease';

  // Icon + Title row
  var header = document.createElement('div');
  header.style.cssText = 'display:flex;align-items:center;gap:12px;margin-bottom:20px';

  var icon = document.createElement('div');
  icon.style.cssText = 'width:44px;height:44px;border-radius:10px;background:rgba(248,113,113,0.15);display:flex;align-items:center;justify-content:center;flex-shrink:0';
  icon.innerHTML = '<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#f87171" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>';

  var title = document.createElement('div');
  title.style.cssText = 'font-size:18px;font-weight:600;color:#f0f0f3';
  title.textContent = 'Extension Required';

  header.appendChild(icon);
  header.appendChild(title);

  // Message
  var msg = document.createElement('p');
  msg.style.cssText = 'font-size:14px;line-height:1.6;color:#b0b0bc;margin-bottom:24px';
  msg.innerHTML = 'Focuser closed <strong style="color:#f0f0f3">{browser_name}</strong> because the Focuser browser extension is not installed.<br><br>To continue using {browser_name} while blocks are active, please install the extension.';

  // Steps
  var steps = document.createElement('div');
  steps.style.cssText = 'background:#141418;border:1px solid rgba(255,255,255,0.06);border-radius:8px;padding:16px;margin-bottom:28px';

  var stepsTitle = document.createElement('div');
  stepsTitle.style.cssText = 'font-size:12px;font-weight:600;text-transform:uppercase;letter-spacing:0.05em;color:#6e6e7a;margin-bottom:12px';
  stepsTitle.textContent = 'How to install';

  var stepsList = document.createElement('div');
  stepsList.style.cssText = 'font-size:13px;color:#b0b0bc;line-height:1.8';
  stepsList.innerHTML = ''
    + '<div style="display:flex;gap:10px;align-items:baseline"><span style="color:#8b5cf6;font-weight:600;font-size:12px">1.</span> Open <strong style="color:#f0f0f3">{browser_name}</strong></div>'
    + '<div style="display:flex;gap:10px;align-items:baseline"><span style="color:#8b5cf6;font-weight:600;font-size:12px">2.</span> Go to the <strong style="color:#f0f0f3">Extensions</strong> page</div>'
    + '<div style="display:flex;gap:10px;align-items:baseline"><span style="color:#8b5cf6;font-weight:600;font-size:12px">3.</span> Enable <strong style="color:#f0f0f3">Developer Mode</strong></div>'
    + '<div style="display:flex;gap:10px;align-items:baseline"><span style="color:#8b5cf6;font-weight:600;font-size:12px">4.</span> Click <strong style="color:#f0f0f3">Load unpacked</strong> and select the extension folder</div>';

  steps.appendChild(stepsTitle);
  steps.appendChild(stepsList);

  // Button
  var btn = document.createElement('button');
  btn.textContent = 'Got it';
  btn.style.cssText = 'width:100%;padding:10px 20px;background:#8b5cf6;color:#fff;border:none;border-radius:8px;font-size:14px;font-weight:600;cursor:pointer;transition:background 0.15s ease;font-family:inherit';
  btn.onmouseenter = function() {{ btn.style.background = '#9d74fa'; }};
  btn.onmouseleave = function() {{ btn.style.background = '#8b5cf6'; }};
  btn.onclick = function() {{ overlay.remove(); }};

  // Assemble
  modal.appendChild(header);
  modal.appendChild(msg);
  modal.appendChild(steps);
  modal.appendChild(btn);
  overlay.appendChild(modal);

  // Add animation keyframes
  var style = document.createElement('style');
  style.textContent = '@keyframes focuserFadeIn {{from{{opacity:0}}to{{opacity:1}}}} @keyframes focuserSlideIn {{from{{opacity:0;transform:scale(0.95) translateY(10px)}}to{{opacity:1;transform:scale(1) translateY(0)}}}}';
  document.head.appendChild(style);

  // Close on overlay click (not modal click)
  overlay.onclick = function(e) {{ if (e.target === overlay) overlay.remove(); }};

  // Close on Escape
  var escHandler = function(e) {{ if (e.key === 'Escape') {{ overlay.remove(); document.removeEventListener('keydown', escHandler); }} }};
  document.addEventListener('keydown', escHandler);

  document.body.appendChild(overlay);
  btn.focus();
}})();"##,
        browser_name = browser_name
    )
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
