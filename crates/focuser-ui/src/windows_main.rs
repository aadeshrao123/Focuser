#[path = "windows_autostart.rs"]
mod autostart;
mod native;
mod sound;
mod typed_commands;
use focuser_app::{Command, CommandErrorPayload, CommandResult};
use focuser_ipc::{Client, Endpoint};
use std::sync::Arc;
use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
pub struct AppState {
    client: Client,
    development: bool,
}

fn dispatch(state: &AppState, command: Command) -> Result<CommandResult, CommandErrorPayload> {
    // Icons are user-session reads and never need LocalSystem or a database.
    if let Command::GetAppIcons { targets } = command {
        let loader = focuser_common::appicon::Loader::new();
        let mut seen = std::collections::HashSet::new();
        return Ok(CommandResult::AppIcons(
            targets
                .into_iter()
                .filter(|t| seen.insert(t.clone()))
                .map(|target| focuser_app::command::AppIcon {
                    data_uri: loader.icon_for(&target),
                    target,
                })
                .collect(),
        ));
    }
    state.client.command(command)
}
fn client() -> Result<Client, String> {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let dev = args.iter().any(|a| a == "--dev");
    let name = value("--dev-pipe");
    let pid = value("--dev-server-pid");
    let endpoint = if dev {
        Endpoint::Development {
            name: name.ok_or("--dev requires --dev-pipe")?,
            server_pid: pid
                .ok_or("--dev requires --dev-server-pid")?
                .parse()
                .map_err(|_| "Invalid server PID")?,
        }
    } else {
        if name.is_some() || pid.is_some() {
            return Err("Development endpoints require --dev".into());
        }
        Endpoint::Production
    };
    endpoint.pipe_name().map_err(|e| e.to_string())?;
    Ok(Client { endpoint })
}
fn main() {
    if std::env::args().any(|a| a == "--export-bindings") {
        std::process::exit(if typed_commands::export_bindings() {
            0
        } else {
            1
        });
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let client = client().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    // Connection failures are returned to React; opening the interface never
    // creates a database, starts a policy engine or launches a replacement.
    let development = matches!(&client.endpoint, Endpoint::Development { .. });
    let state = Arc::new(AppState {
        client,
        development,
    });
    // Scratch clients must not hand off to an open production interface.
    let builder = tauri::Builder::default();
    let builder = if development {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
    };
    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            typed_commands::run_command,
            native::pick_app_file,
            native::pick_import_file,
            native::save_configuration,
            native::open_browser_url,
            native::check_for_update,
            native::do_update,
            native::update_status,
            native::cancel_update,
            autostart::is_autostart_enabled,
            autostart::set_autostart,
            sound::pick_sound_file,
            sound::preview_sound
        ])
        .setup(|app| {
            let state = app.state::<Arc<AppState>>().inner().clone();
            if !state.development {
                let handle = app.handle().clone();
                std::thread::Builder::new()
                    .name("focuser-update-observer".into())
                    .spawn(move || {
                        loop {
                            if state.client.removal_shutdown_requested() {
                                handle.exit(0);
                                break;
                            }
                            if let Ok(focuser_ipc::ReplyBody::Update(status)) = state
                                .client
                                .request_blocking(focuser_ipc::RequestBody::Update(
                                    focuser_ipc::UpdateRequest::Status {},
                                ))
                                && status.shutdown_clients()
                            {
                                handle.exit(0);
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    })?;
            }
            let open = MenuItemBuilder::with_id("show", "Open Focuser").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Close interface").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Focuser — service client")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        show(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if !std::env::args().any(|a| a == "--autostart") {
                show(app.handle());
            }
            let handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(window) = handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Focuser interface failed");
}
fn show(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
