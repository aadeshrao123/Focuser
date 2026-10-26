//! Focuser CLI: production Windows commands use the service authority.
//! `--dev --db <scratch>` is an explicit, non-enforcing test harness.

mod args;
mod output;
mod password;
mod slot;

#[cfg(not(windows))]
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
#[cfg(not(windows))]
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
        Err(Failure::Remote(e)) => {
            eprintln!("{}: {}", e.code, e.message);
            ExitCode::from(match e.code.as_str() {
                "validation" => 2,
                "block_list_not_found" | "rule_not_found" | "allowance_not_found" => 4,
                "wrong_unlock_response" => 3,
                "protected" => 5,
                "unsupported" => 6,
                _ => 1,
            })
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
    Remote(focuser_app::CommandErrorPayload),
}

impl From<anyhow::Error> for Failure {
    fn from(e: anyhow::Error) -> Self {
        Self::Startup(e)
    }
}

fn run(cli: Cli) -> Result<(), Failure> {
    if let args::TopLevel::ServiceUpdate {
        action,
        transaction,
    } = &cli.command
    {
        if cli.dev {
            return Err(Failure::Startup(anyhow::anyhow!(
                "Development clients cannot activate production updates"
            )));
        }
        use focuser_ipc::{ReplyBody, RequestBody, UpdateRequest as R};
        let id = || {
            transaction
                .clone()
                .ok_or_else(|| Failure::Startup(anyhow::anyhow!("--transaction is required")))
        };
        let request = match action {
            args::UpdateAction::Status => R::Status {},
            args::UpdateAction::Check => R::Check {},
            args::UpdateAction::Stage => R::Stage {},
            args::UpdateAction::Activate => R::Activate { transaction: id()? },
            args::UpdateAction::Cancel => R::Cancel { transaction: id()? },
        };
        let reply = focuser_ipc::Client::default()
            .request_blocking(RequestBody::Update(request))
            .map_err(|e| Failure::Remote(e.payload()))?;
        if let ReplyBody::Update(status) = reply {
            println!(
                "{}",
                serde_json::to_string(&status).map_err(|e| Failure::Startup(e.into()))?
            );
            return Ok(());
        }
        return Err(Failure::Startup(anyhow::anyhow!("Unexpected update reply")));
    }
    let command = cli.command.into_command().map_err(Failure::from)?;

    let result = if cli.dev {
        if let Some(name) = cli.dev_pipe {
            let client = focuser_ipc::Client {
                endpoint: focuser_ipc::Endpoint::Development {
                    name,
                    server_pid: cli.dev_server_pid.ok_or_else(|| {
                        Failure::Startup(anyhow::anyhow!("--dev-server-pid is required"))
                    })?,
                },
            };
            client.command(command).map_err(Failure::Remote)?
        } else {
            let path = cli.db.ok_or_else(|| {
                Failure::Startup(anyhow::anyhow!("--dev requires --db or --dev-pipe"))
            })?;
            let db_path = focuser_ipc::paths::ensure_development_database(&path)
                .map_err(|e| Failure::Startup(e.into()))?;
            let db = Database::open(&db_path).map_err(|e| Failure::Startup(e.into()))?;
            let engine = BlockEngine::new(db).map_err(|e| Failure::Startup(e.into()))?;
            let ctx = Arc::new(AppContext::new_headless(engine));
            execute(&ctx, command).map_err(Failure::Command)?
        }
    } else {
        #[cfg(windows)]
        {
            focuser_ipc::Client::default()
                .command(command)
                .map_err(Failure::Remote)?
        }
        #[cfg(not(windows))]
        {
            let db = Database::open(default_db_path().map_err(Failure::from)?)
                .map_err(|e| Failure::Startup(e.into()))?;
            let engine = BlockEngine::new(db).map_err(|e| Failure::Startup(e.into()))?;
            execute(&Arc::new(AppContext::new_headless(engine)), command)
                .map_err(Failure::Command)?
        }
    };

    if cli.json {
        output::print_json(&result).map_err(Failure::from)?;
    } else {
        output::print_human(&result);
    }

    Ok(())
}

/// The database the desktop app uses.
#[cfg(not(windows))]
fn default_db_path() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "focuser", "Focuser")
        .ok_or_else(|| anyhow::anyhow!("could not determine the Focuser data directory"))?;

    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir)?;
    Ok(dir.join("focuser.db"))
}
