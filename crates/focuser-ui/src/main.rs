#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod blocker;

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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Focuser starting");

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
            let state = Arc::clone(&state_for_blocker);
            std::thread::spawn(move || {
                blocker::run_blocking_loop(state);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Focuser");
}
