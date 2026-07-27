//! Focuser CLI.
//!
//! A thin shim over `focuser_app::execute` — the same function the Tauri GUI and
//! the dev server call. Every state-changing operation the app can perform is
//! therefore reachable from a script, and cannot drift from the GUI, because
//! there is only one implementation.
//!
//! Two properties make this usable as a test harness:
//!
//! - `--db <path>` runs against a scratch database, so exercising commands never
//!   touches real block lists.
//! - Failures exit non-zero, with a distinct code per failure kind
//!   (2 validation, 4 not found, 5 protected, 6 unsupported, 1 internal). The
//!   previous CLI returned `Ok(())` on every path, so no script could detect a
//!   failure at all.
//!
//! ```bash
//! focuser --db /tmp/test.db list create "Social"
//! focuser --db /tmp/test.db --json list ls
//! ```
//!
//! This replaces an IPC-based CLI that talked to `focuser-service` over TCP
//! 17549. That port is contested by the Tauri app's extension API, and nothing
//! ever launched the service, so those commands could not work in a shipped
//! install. Going straight to the database removes the moving part entirely.

mod args;
mod output;
mod slot;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use directories::ProjectDirs;
use focuser_app::{AppContext, CommandError, execute};
use focuser_core::{BlockEngine, Database};

use crate::args::Cli;

/// Used when the CLI cannot get as far as running a command. Bad arguments are
/// already rejected by clap, so this covers I/O and database failures.
const EXIT_STARTUP_FAILURE: u8 = 1;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Startup(e)) => {
            eprintln!("error: {e:#}");
            ExitCode::from(EXIT_STARTUP_FAILURE)
        }
        Err(Failure::Command(e)) => {
            // Machine-readable code first, then the human explanation.
            eprintln!("{}: {e}", e.code());
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

enum Failure {
    Startup(anyhow::Error),
    Command(CommandError),
}

impl From<anyhow::Error> for Failure {
    fn from(e: anyhow::Error) -> Self {
        Self::Startup(e)
    }
}

fn run(cli: Cli) -> Result<(), Failure> {
    let command = cli.command.into_command().map_err(Failure::from)?;

    let db_path = match cli.db {
        Some(path) => path,
        None => default_db_path().map_err(Failure::from)?,
    };

    let db = Database::open(&db_path)
        .map_err(|e| Failure::Startup(anyhow::anyhow!("cannot open {}: {e}", db_path.display())))?;
    let engine = BlockEngine::new(db)
        .map_err(|e| Failure::Startup(anyhow::anyhow!("cannot initialise engine: {e}")))?;

    // Headless: the CLI does not rewrite the hosts file as a side effect of
    // every command. `focuser blocks apply` is the explicit way to do that.
    let ctx = Arc::new(AppContext::new_headless(engine));

    let result = execute(&ctx, command).map_err(Failure::Command)?;

    if cli.json {
        output::print_json(&result).map_err(Failure::from)?;
    } else {
        output::print_human(&result);
    }

    Ok(())
}

/// The database the desktop app uses.
fn default_db_path() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "focuser", "Focuser")
        .ok_or_else(|| anyhow::anyhow!("could not determine the Focuser data directory"))?;

    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir)?;
    Ok(dir.join("focuser.db"))
}
