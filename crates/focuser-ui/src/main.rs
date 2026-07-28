#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod blocker;
mod foreground_watcher;
mod native;
mod typed_commands;

use directories::ProjectDirs;

use focuser_common::types::AppMatchType;
use focuser_core::{BlockEngine, Database};
use std::sync::Arc;
use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

/// Shared application state.
///
/// This *is* `focuser_app::AppContext` — the GUI no longer owns a private copy of
/// the engine handle. Kept as an alias so the 70-odd existing `AppState`
/// references keep compiling while commands are ported over domain by domain.
///
/// New code should say `AppContext`.
pub use focuser_app::{AppContext as AppState, PomodoroEvent};

/// Pushes the engine's blocked-domain set into the system hosts file.
///
/// This is the GUI's implementation of the one system side effect the command
/// core needs. Injecting it keeps `focuser-app` free of any dependency on a
/// particular frontend, and lets tests substitute a no-op.
struct HostsSync;

impl focuser_app::SystemSync for HostsSync {
    fn sync_hosts(&self, domains: &[String]) {
        let _ = blocker::apply_hosts_blocks(domains);
    }

    fn running_browsers(&self) -> Vec<String> {
        native::detect_running_browsers()
            .iter()
            .map(|b| format!("{b:?}"))
            .collect()
    }

    fn connected_browsers(&self) -> Vec<String> {
        api::get_connected_browsers(EXTENSION_SEEN_SECS)
            .iter()
            .map(|b| format!("{b:?}"))
            .collect()
    }

    fn hosts_writable(&self) -> bool {
        blocker::hosts_writable()
    }
}

/// How recently an extension must have checked in to count as connected.
const EXTENSION_SEEN_SECS: u64 = 120;

fn main() {
    // Regenerate frontend TypeScript bindings and exit. Handled before any
    // logging, database access, or window creation so it works in CI and in a
    // checkout where another Focuser instance already holds the database.
    if std::env::args().any(|a| a == "--export-bindings") {
        std::process::exit(if typed_commands::export_bindings() {
            0
        } else {
            1
        });
    }

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

    let state = Arc::new(AppState::new(engine, Arc::new(HostsSync)));

    let state_for_blocker = Arc::clone(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Another instance tried to launch — bring existing window to front
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // Everything the application can do goes through one command.
            typed_commands::run_command,
            // Native shims that need a Tauri handle or OS process access.
            native::pick_app_file,
            native::pick_import_file,
            native::save_configuration,
            native::open_browser_url,
            native::check_for_update,
            native::do_update,
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

            // Spawn foreground-app watcher — feeds app allowance ticks.
            let watcher_state = Arc::clone(&state_for_blocker);
            std::thread::spawn(move || {
                foreground_watcher::run_foreground_watcher(watcher_state);
            });

            // Warm the icon cache so the Applications page does not pay for the
            // Start Menu search on first open.
            let icon_state = Arc::clone(&state_for_blocker);
            std::thread::spawn(move || warm_app_icons(&icon_state));

            // System tray icon
            let show = MenuItemBuilder::with_id("show", "Open Focuser").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

            let icon = app.default_window_icon().cloned().unwrap();

            let quit_state = Arc::clone(&state_for_blocker);

            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Focuser — Blocking active")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        // Quitting removes the hosts entries and stops the
                        // blocking loop, so while a lock asks us to stay put
                        // this is the whole product being switched off in two
                        // clicks. Refuse and show what is holding it.
                        if let Some(reason) = quit_blocked_by(&quit_state) {
                            warn!(%reason, "Refused to quit — a lock is active");
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.eval(build_locked_modal_js(&reason));
                            }
                            return;
                        }
                        let _ = crate::blocker::remove_hosts_blocks();
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
        .build(tauri::generate_context!())
        .expect("error while building Focuser")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Always clean up hosts file when the app exits
                let _ = blocker::remove_hosts_blocks();
            }
        });
}

/// Get the extension store URL for a given browser.
fn extension_store_url(browser_name: &str) -> (&'static str, &'static str) {
    match browser_name {
        "Mozilla Firefox" => (
            "https://addons.mozilla.org/en-US/firefox/addon/focuser-website-blocker/",
            "firefox",
        ),
        _ => (
            "https://chromewebstore.google.com/detail/jpnhbpbcmagoonmaleppldmcnaibkbmj",
            "chrome",
        ),
    }
}

/// Get the browser executable for launching with a URL.
fn browser_launch_cmd(browser_name: &str) -> &'static str {
    match browser_name {
        "Mozilla Firefox" => "firefox",
        "Microsoft Edge" => "msedge",
        "Brave Browser" => "brave",
        "Opera" => "opera",
        _ => "chrome",
    }
}

/// Build JavaScript to inject a themed modal into the Focuser UI.
/// A small overlay explaining why Quit did nothing.
///
/// Injected rather than routed through the frontend because the tray menu can
/// be used while the window has never been opened, so there may be no React
/// tree listening yet.
fn build_locked_modal_js(reason: &str) -> String {
    // The reason carries a user-chosen list name, so it goes in as JSON rather
    // than being pasted into the source.
    let reason = serde_json::to_string(reason).unwrap_or_else(|_| "\"a lock is active\"".into());
    format!(
        r##"(function() {{
  var id = 'focuser-locked-overlay';
  var old = document.getElementById(id);
  if (old) old.remove();

  var overlay = document.createElement('div');
  overlay.id = id;
  overlay.style.cssText = 'position:fixed;inset:0;z-index:99999;display:flex;' +
    'align-items:center;justify-content:center;background:rgba(0,0,0,.55);' +
    'backdrop-filter:blur(4px);font-family:system-ui,sans-serif';

  var card = document.createElement('div');
  card.style.cssText = 'max-width:26rem;margin:1.5rem;padding:1.5rem;border-radius:.75rem;' +
    'background:#16181d;color:#e8eaed;border:1px solid #2c3038;box-shadow:0 18px 50px rgba(0,0,0,.5)';

  var title = document.createElement('h2');
  title.textContent = 'Focuser is locked';
  title.style.cssText = 'margin:0 0 .5rem;font-size:1rem;font-weight:600';

  var body = document.createElement('p');
  body.textContent = 'You asked Focuser to stay running until this lock ends, so it will not quit yet: ' + {reason} + '.';
  body.style.cssText = 'margin:0 0 1.25rem;font-size:.875rem;line-height:1.5;color:#9aa0aa';

  var button = document.createElement('button');
  button.textContent = 'OK';
  button.style.cssText = 'padding:.45rem 1.1rem;border-radius:.4rem;border:0;cursor:pointer;' +
    'background:#4f7cff;color:#fff;font-size:.875rem';
  button.onclick = function() {{ overlay.remove(); }};

  card.appendChild(title);
  card.appendChild(body);
  card.appendChild(button);
  overlay.appendChild(card);
  document.body.appendChild(overlay);
}})();"##
    )
}

/// Reads every application rule's icon once at startup, so opening the
/// Applications page is a cache hit rather than a Start Menu search.
fn warm_app_icons(state: &Arc<AppState>) {
    let Ok(engine) = state.engine.lock() else {
        return;
    };
    let targets: Vec<String> = engine
        .block_lists()
        .iter()
        .flat_map(|list| &list.applications)
        .map(|rule| match &rule.match_type {
            AppMatchType::ExecutableName(v)
            | AppMatchType::ExecutablePath(v)
            | AppMatchType::WindowTitle(v)
            | AppMatchType::BundleId(v) => v.clone(),
        })
        .collect();
    drop(engine);

    let loader = focuser_common::appicon::Loader::new();
    for target in &targets {
        loader.icon_for(target);
    }
}

/// Why quitting is refused right now, or `None` if it is allowed.
///
/// Only locks that asked to prevent it count. A lock set without that box
/// ticked is a commitment about the block list, not about the app staying up.
fn quit_blocked_by(state: &Arc<AppState>) -> Option<String> {
    let engine = state.engine.lock().ok()?;
    let list = engine
        .block_lists()
        .iter()
        .filter(|l| l.has_service_protection())
        .max_by_key(|l| l.protection.as_ref().map_or(0, |p| p.remaining_seconds()))?;

    let remaining = list.protection.as_ref()?.remaining_seconds();
    Some(format!(
        "{} — {} left",
        list.name,
        format_remaining(remaining)
    ))
}

/// Coarse, human phrasing. The exact second does not matter to someone being
/// told they cannot quit yet.
fn format_remaining(secs: u64) -> String {
    let mins = secs.div_ceil(60);
    match mins {
        0 => "less than a minute".into(),
        1 => "1 minute".into(),
        m if m < 60 => format!("{m} minutes"),
        m => {
            let (h, rem) = (m / 60, m % 60);
            let hours = if h == 1 {
                "1 hour".into()
            } else {
                format!("{h} hours")
            };
            if rem == 0 {
                hours
            } else {
                format!("{hours} {rem} min")
            }
        }
    }
}

fn build_extension_modal_js(browser_name: &str) -> String {
    let (store_url, store_type) = extension_store_url(browser_name);
    let browser_exe = browser_launch_cmd(browser_name);
    let store_label = if store_type == "firefox" {
        "Firefox Add-ons"
    } else {
        "Chrome Web Store"
    };

    format!(
        r##"(function() {{
  var old = document.getElementById('focuser-ext-modal-overlay');
  if (old) old.remove();

  var overlay = document.createElement('div');
  overlay.id = 'focuser-ext-modal-overlay';
  overlay.style.cssText = 'position:fixed;top:0;left:0;width:100%;height:100%;background:rgba(0,0,0,0.6);backdrop-filter:blur(4px);z-index:99999;display:flex;align-items:center;justify-content:center;animation:focuserFadeIn 0.2s ease';

  var modal = document.createElement('div');
  modal.style.cssText = 'background:#1e1e24;border:1px solid rgba(255,255,255,0.1);border-radius:12px;padding:32px;max-width:480px;width:90%;box-shadow:0 8px 32px rgba(0,0,0,0.6);font-family:Inter,-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;color:#f0f0f3;animation:focuserSlideIn 0.25s ease';

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

  var msg = document.createElement('p');
  msg.style.cssText = 'font-size:14px;line-height:1.6;color:#b0b0bc;margin-bottom:24px';
  msg.innerHTML = 'Focuser closed <strong style="color:#f0f0f3">{browser_name}</strong> because the Focuser browser extension is not installed.<br><br>Install the extension from the <strong style="color:#f0f0f3">{store_label}</strong> to continue using {browser_name} while blocks are active.';

  var btnRow = document.createElement('div');
  btnRow.style.cssText = 'display:flex;gap:12px;flex-direction:column';

  var installBtn = document.createElement('button');
  installBtn.style.cssText = 'width:100%;padding:12px 20px;background:#8b5cf6;color:#fff;border:none;border-radius:8px;font-size:14px;font-weight:600;cursor:pointer;transition:all 0.15s ease;font-family:inherit;display:flex;align-items:center;justify-content:center;gap:8px';
  installBtn.innerHTML = '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg> Install Extension for {browser_name}';
  installBtn.onmouseenter = function() {{ installBtn.style.background = '#9d74fa'; installBtn.style.transform = 'translateY(-1px)'; }};
  installBtn.onmouseleave = function() {{ installBtn.style.background = '#8b5cf6'; installBtn.style.transform = 'translateY(0)'; }};
  installBtn.onclick = function() {{
    var cmd = '{browser_exe}';
    var url = '{store_url}';
    try {{
      window.__TAURI__.core.invoke('open_browser_url', {{ browser: cmd, url: url }})
        .catch(function(err) {{ console.error('Focuser: invoke failed:', err); }});
    }} catch(e) {{ console.error('Focuser: catch:', e); }}
    overlay.remove();
  }};

  var dismissBtn = document.createElement('button');
  dismissBtn.textContent = 'Dismiss';
  dismissBtn.style.cssText = 'width:100%;padding:10px 20px;background:transparent;color:#6e6e7a;border:1px solid rgba(255,255,255,0.08);border-radius:8px;font-size:13px;font-weight:500;cursor:pointer;transition:all 0.15s ease;font-family:inherit';
  dismissBtn.onmouseenter = function() {{ dismissBtn.style.color = '#b0b0bc'; dismissBtn.style.borderColor = 'rgba(255,255,255,0.15)'; }};
  dismissBtn.onmouseleave = function() {{ dismissBtn.style.color = '#6e6e7a'; dismissBtn.style.borderColor = 'rgba(255,255,255,0.08)'; }};
  dismissBtn.onclick = function() {{ overlay.remove(); }};

  btnRow.appendChild(installBtn);
  btnRow.appendChild(dismissBtn);

  modal.appendChild(header);
  modal.appendChild(msg);
  modal.appendChild(btnRow);
  overlay.appendChild(modal);

  var style = document.createElement('style');
  style.textContent = '@keyframes focuserFadeIn {{from{{opacity:0}}to{{opacity:1}}}} @keyframes focuserSlideIn {{from{{opacity:0;transform:scale(0.95) translateY(10px)}}to{{opacity:1;transform:scale(1) translateY(0)}}}}';
  document.head.appendChild(style);

  overlay.onclick = function(e) {{ if (e.target === overlay) overlay.remove(); }};
  var escHandler = function(e) {{ if (e.key === 'Escape') {{ overlay.remove(); document.removeEventListener('keydown', escHandler); }} }};
  document.addEventListener('keydown', escHandler);

  document.body.appendChild(overlay);
  installBtn.focus();
}})();"##,
        browser_name = browser_name,
        store_label = store_label,
        store_url = store_url,
        browser_exe = browser_exe,
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

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_common::types::{BlockList, Protection};
    use focuser_core::Database;

    fn state_with(list: BlockList) -> Arc<AppState> {
        let db = Database::open_in_memory().unwrap();
        db.create_block_list(&list).unwrap();
        let engine = BlockEngine::new(db).unwrap();
        Arc::new(AppState::new_headless(engine))
    }

    fn locked(prevent_service_stop: bool, minutes: i64) -> BlockList {
        let mut list = BlockList::new("Deep Work");
        list.enabled = true;
        let now = chrono::Utc::now();
        list.protection = Some(Protection {
            prevent_uninstall: false,
            prevent_service_stop,
            prevent_modification: false,
            started_at: now,
            expires_at: now + chrono::Duration::minutes(minutes),
        });
        list
    }

    #[test]
    fn quitting_is_refused_while_a_lock_asks_us_to_stay() {
        let reason = quit_blocked_by(&state_with(locked(true, 45)));
        let reason = reason.expect("an active lock should hold the app open");
        assert!(
            reason.contains("Deep Work"),
            "should name the list: {reason}"
        );
        assert!(
            reason.contains("45 minutes"),
            "should say how long: {reason}"
        );
    }

    #[test]
    fn a_lock_that_did_not_ask_to_prevent_quitting_does_not() {
        // Locking a list is a commitment about that list. Only the explicit
        // checkbox turns it into a commitment about the app staying up.
        assert!(quit_blocked_by(&state_with(locked(false, 45))).is_none());
    }

    #[test]
    fn an_expired_lock_releases_the_app() {
        assert!(quit_blocked_by(&state_with(locked(true, -1))).is_none());
    }

    #[test]
    fn a_disabled_list_holds_nothing() {
        let mut list = locked(true, 45);
        list.enabled = false;
        assert!(quit_blocked_by(&state_with(list)).is_none());
    }

    #[test]
    fn with_no_lists_at_all_quitting_is_allowed() {
        let db = Database::open_in_memory().unwrap();
        let engine = BlockEngine::new(db).unwrap();
        assert!(quit_blocked_by(&Arc::new(AppState::new_headless(engine))).is_none());
    }

    #[test]
    fn remaining_time_reads_naturally_at_every_scale() {
        assert_eq!(format_remaining(0), "less than a minute");
        assert_eq!(format_remaining(30), "1 minute");
        assert_eq!(format_remaining(60), "1 minute");
        assert_eq!(format_remaining(90), "2 minutes");
        assert_eq!(format_remaining(3600), "1 hour");
        assert_eq!(format_remaining(5400), "1 hour 30 min");
        assert_eq!(format_remaining(7200), "2 hours");
    }
}
