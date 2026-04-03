mod hosts;
mod ipc;
mod platform;
mod service;

use anyhow::Result;
use directories::ProjectDirs;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    info!("Focuser service starting");

    // Determine data directory
    let project_dirs = ProjectDirs::from("com", "focuser", "Focuser")
        .expect("Could not determine project directories");
    let data_dir = project_dirs.data_dir();
    std::fs::create_dir_all(data_dir)?;

    let db_path = data_dir.join("focuser.db");
    info!(path = %db_path.display(), "Opening database");

    // Initialize database and engine
    let db = focuser_core::Database::open(&db_path)?;
    let engine = focuser_core::BlockEngine::new(db)?;

    // Start the service
    let mut svc = service::FocuserService::new(engine)?;
    svc.run().await?;

    Ok(())
}
