mod client;

use anyhow::Result;
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use focuser_common::ipc::{IpcRequest, IpcResponse};
use focuser_common::types::*;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "focuser", about = "Focuser — website and app blocker", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check if the service is running.
    Ping,

    /// Show service status.
    Status,

    /// Manage block lists.
    #[command(subcommand)]
    List(ListCommands),

    /// Check if a domain is blocked.
    Check {
        /// Domain to check.
        domain: String,
    },

    /// Show blocked attempt statistics.
    Stats {
        /// Start date (YYYY-MM-DD). Defaults to today.
        #[arg(long)]
        from: Option<String>,
        /// End date (YYYY-MM-DD). Defaults to today.
        #[arg(long)]
        to: Option<String>,
    },

    /// Stop the service.
    Shutdown,
}

#[derive(Subcommand)]
enum ListCommands {
    /// Show all block lists.
    Show,

    /// Create a new block list.
    Create {
        /// Name of the block list.
        name: String,
    },

    /// Delete a block list.
    Delete {
        /// ID of the block list.
        id: String,
    },

    /// Add a website domain to a block list.
    AddSite {
        /// Block list ID.
        list_id: String,
        /// Domain to block (e.g., "reddit.com").
        domain: String,
    },

    /// Add an application to a block list.
    AddApp {
        /// Block list ID.
        list_id: String,
        /// Executable name (e.g., "steam.exe").
        exe: String,
    },

    /// Enable a block list.
    Enable {
        /// Block list ID.
        id: String,
    },

    /// Disable a block list.
    Disable {
        /// Block list ID.
        id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Ping => match client::send(IpcRequest::Ping).await {
            Ok(IpcResponse::Pong) => println!("Service is running."),
            Ok(other) => println!("Unexpected response: {other:?}"),
            Err(e) => println!("Service is NOT running: {e}"),
        },

        Commands::Status => match client::send(IpcRequest::GetStatus).await {
            Ok(IpcResponse::Status(status)) => {
                println!("Focuser Service Status");
                println!("══════════════════════");
                println!("Running:          {}", status.running);
                println!("Uptime:           {}s", status.uptime_seconds);
                println!("Blocked today:    {}", status.total_blocked_today);
                println!("Active blocks:    {}", status.active_blocks.len());
                for block in &status.active_blocks {
                    println!(
                        "  • {} — {} sites, {} apps",
                        block.block_list_name, block.blocked_websites, block.blocked_apps
                    );
                }
            }
            Ok(other) => println!("Unexpected: {other:?}"),
            Err(e) => eprintln!("Error: {e}"),
        },

        Commands::List(sub) => match sub {
            ListCommands::Show => match client::send(IpcRequest::ListBlockLists).await {
                Ok(IpcResponse::BlockLists(lists)) => {
                    if lists.is_empty() {
                        println!("No block lists. Create one with: focuser list create <name>");
                    } else {
                        for list in &lists {
                            let status = if list.enabled { "ON" } else { "OFF" };
                            println!(
                                "[{status}] {} (id: {})",
                                list.name,
                                &list.id.to_string()[..8]
                            );
                            for site in &list.websites {
                                println!("      web: {:?}", site.match_type);
                            }
                            for app in &list.applications {
                                println!("      app: {:?}", app.match_type);
                            }
                        }
                    }
                }
                Ok(other) => println!("Unexpected: {other:?}"),
                Err(e) => eprintln!("Error: {e}"),
            },

            ListCommands::Create { name } => {
                let list = BlockList::new(&name);
                let id = list.id;
                match client::send(IpcRequest::CreateBlockList(list)).await {
                    Ok(IpcResponse::Ok) => {
                        println!(
                            "Created block list \"{name}\" (id: {})",
                            &id.to_string()[..8]
                        );
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                    Ok(other) => println!("Unexpected: {other:?}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }

            ListCommands::Delete { id } => {
                let uuid = parse_id(&id)?;
                match client::send(IpcRequest::DeleteBlockList(uuid)).await {
                    Ok(IpcResponse::Ok) => println!("Deleted."),
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                    Ok(other) => println!("Unexpected: {other:?}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }

            ListCommands::AddSite { list_id, domain } => {
                let uuid = parse_id(&list_id)?;
                match client::send(IpcRequest::GetBlockList(uuid)).await {
                    Ok(IpcResponse::BlockList(mut list)) => {
                        list.websites.push(WebsiteRule::domain(&domain));
                        list.updated_at = chrono::Utc::now();
                        match client::send(IpcRequest::UpdateBlockList(list)).await {
                            Ok(IpcResponse::Ok) => println!("Added {domain} to block list."),
                            Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                            _ => {}
                        }
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                    Ok(other) => println!("Unexpected: {other:?}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }

            ListCommands::AddApp { list_id, exe } => {
                let uuid = parse_id(&list_id)?;
                match client::send(IpcRequest::GetBlockList(uuid)).await {
                    Ok(IpcResponse::BlockList(mut list)) => {
                        list.applications.push(AppRule::executable(&exe));
                        list.updated_at = chrono::Utc::now();
                        match client::send(IpcRequest::UpdateBlockList(list)).await {
                            Ok(IpcResponse::Ok) => println!("Added {exe} to block list."),
                            Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                            _ => {}
                        }
                    }
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                    Ok(other) => println!("Unexpected: {other:?}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }

            ListCommands::Enable { id } => {
                let uuid = parse_id(&id)?;
                match client::send(IpcRequest::SetBlockListEnabled {
                    id: uuid,
                    enabled: true,
                })
                .await
                {
                    Ok(IpcResponse::Ok) => println!("Block list enabled."),
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                    Ok(other) => println!("Unexpected: {other:?}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }

            ListCommands::Disable { id } => {
                let uuid = parse_id(&id)?;
                match client::send(IpcRequest::SetBlockListEnabled {
                    id: uuid,
                    enabled: false,
                })
                .await
                {
                    Ok(IpcResponse::Ok) => println!("Block list disabled."),
                    Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                    Ok(other) => println!("Unexpected: {other:?}"),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
        },

        Commands::Check { domain } => {
            match client::send(IpcRequest::CheckDomain(domain.clone())).await {
                Ok(IpcResponse::DomainBlocked(true)) => println!("{domain} is BLOCKED"),
                Ok(IpcResponse::DomainBlocked(false)) => println!("{domain} is not blocked"),
                Ok(other) => println!("Unexpected: {other:?}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }

        Commands::Stats { from, to } => {
            let today = chrono::Utc::now().date_naive();
            let from_date = from
                .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
                .unwrap_or(today);
            let to_date = to
                .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
                .unwrap_or(today);

            match client::send(IpcRequest::GetStats {
                from: from_date,
                to: to_date,
            })
            .await
            {
                Ok(IpcResponse::Stats(stats)) => {
                    if stats.is_empty() {
                        println!("No stats for this period.");
                    } else {
                        println!("Domain/App            Blocked  Duration");
                        println!("────────────────────  ───────  ────────");
                        for s in &stats {
                            println!(
                                "{:<22} {:>5}    {:>5}s",
                                s.domain_or_app, s.blocked_attempts, s.duration_seconds
                            );
                        }
                    }
                }
                Ok(other) => println!("Unexpected: {other:?}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }

        Commands::Shutdown => match client::send(IpcRequest::Shutdown).await {
            Ok(_) => println!("Service shutting down."),
            Err(e) => eprintln!("Error: {e}"),
        },
    }

    Ok(())
}

fn parse_id(s: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|e| anyhow::anyhow!("Invalid ID \"{s}\": {e}"))
}
