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

    /// Show detected browsers and extension status.
    Browsers,

    /// Enable focus protection on a block list.
    Protect {
        /// Block list ID.
        id: String,
        /// Duration in minutes.
        #[arg(long, default_value = "60")]
        duration: u32,
        /// Prevent uninstallation.
        #[arg(long, default_value = "true")]
        uninstall: bool,
        /// Prevent service stop.
        #[arg(long, default_value = "true")]
        service: bool,
        /// Prevent block list modification.
        #[arg(long, default_value = "true")]
        modify: bool,
    },

    /// Show protection status for all block lists.
    ProtectionStatus,

    /// Stop the service.
    Shutdown,

    /// Show current Pomodoro focus session status (queries the desktop app).
    PomodoroStatus,

    /// List today's allowances with remaining time (queries the desktop app).
    Allowances,
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

        Commands::Browsers => match client::send(IpcRequest::GetBrowserStatus).await {
            Ok(IpcResponse::BrowserStatus(statuses)) => {
                println!("Browser Status");
                println!("══════════════════════════════════════════════════════");
                for s in &statuses {
                    let running = if s.is_running { "RUNNING" } else { "       " };
                    let ext = if s.extension_connected {
                        "CONNECTED".to_string()
                    } else if s.is_running {
                        match s.grace_period_remaining_secs {
                            Some(secs) => format!("MISSING (grace: {secs}s)"),
                            None => "MISSING".to_string(),
                        }
                    } else {
                        "-".to_string()
                    };
                    println!(
                        "  {:<18} {:<10} Extension: {}",
                        s.display_name, running, ext
                    );
                }
            }
            Ok(other) => println!("Unexpected: {other:?}"),
            Err(e) => eprintln!("Error: {e}"),
        },

        Commands::Protect {
            id,
            duration,
            uninstall,
            service,
            modify,
        } => {
            let uuid = parse_id(&id)?;
            match client::send(IpcRequest::EnableProtection {
                block_list_id: uuid,
                duration_minutes: duration,
                prevent_uninstall: uninstall,
                prevent_service_stop: service,
                prevent_modification: modify,
            })
            .await
            {
                Ok(IpcResponse::Ok) => {
                    println!("Protection enabled for {duration} minutes.");
                    if uninstall {
                        println!("  Uninstall prevention: ON");
                    }
                    if service {
                        println!("  Service stop prevention: ON");
                    }
                    if modify {
                        println!("  Modification prevention: ON");
                    }
                }
                Ok(IpcResponse::Error(e)) => eprintln!("Error: {e}"),
                Ok(other) => println!("Unexpected: {other:?}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }

        Commands::ProtectionStatus => match client::send(IpcRequest::GetProtectionStatus).await {
            Ok(IpcResponse::ProtectionStatus(infos)) => {
                if infos.is_empty() {
                    println!("No active protections.");
                } else {
                    println!("Active Protections");
                    println!("══════════════════════════════════════════");
                    for p in &infos {
                        let mins = p.remaining_seconds / 60;
                        let secs = p.remaining_seconds % 60;
                        println!(
                            "  {} ({})",
                            p.block_list_name,
                            &p.block_list_id.to_string()[..8]
                        );
                        println!("    Expires in: {}m {}s", mins, secs);
                        println!(
                            "    Uninstall:  {}",
                            if p.prevent_uninstall {
                                "BLOCKED"
                            } else {
                                "allowed"
                            }
                        );
                        println!(
                            "    Service:    {}",
                            if p.prevent_service_stop {
                                "BLOCKED"
                            } else {
                                "allowed"
                            }
                        );
                        println!(
                            "    Modify:     {}",
                            if p.prevent_modification {
                                "BLOCKED"
                            } else {
                                "allowed"
                            }
                        );
                    }
                }
            }
            Ok(other) => println!("Unexpected: {other:?}"),
            Err(e) => eprintln!("Error: {e}"),
        },

        Commands::Shutdown => match client::send(IpcRequest::Shutdown).await {
            Ok(_) => println!("Service shutting down."),
            Err(e) => eprintln!("Error: {e}"),
        },

        Commands::PomodoroStatus => match http_get("/api/pomodoro") {
            Ok(body) if body.trim() == "null" => println!("No active Pomodoro session."),
            Ok(body) => {
                let v: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                let phase = v["current_phase"].as_str().unwrap_or("?");
                let remaining = v["remaining_secs"].as_u64().unwrap_or(0);
                let cycle = v["current_cycle"].as_u64().unwrap_or(0);
                let completed = v["completed_cycles"].as_u64().unwrap_or(0);
                let block_list = v["block_list_name"].as_str().unwrap_or("?");
                let paused = v["paused"].as_bool().unwrap_or(false);
                let mins = remaining / 60;
                let secs = remaining % 60;
                println!("Pomodoro Status");
                println!("══════════════════════════════════════");
                println!(
                    "Phase:       {phase}{}",
                    if paused { " (paused)" } else { "" }
                );
                println!("Remaining:   {mins:02}:{secs:02}");
                println!("Cycle:       {cycle} · {completed} completed");
                println!("Block list:  {block_list}");
            }
            Err(e) => eprintln!("Could not reach Focuser desktop app (is it running?): {e}"),
        },

        Commands::Allowances => match http_get("/api/allowances") {
            Ok(body) => {
                let list: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap_or_default();
                if list.is_empty() {
                    println!("No allowances configured.");
                } else {
                    println!("Today's Allowances");
                    println!("══════════════════════════════════════════════════════");
                    for item in &list {
                        let a = &item["allowance"];
                        let target = a["target"]["value"].as_str().unwrap_or("?");
                        let kind = a["target"]["kind"].as_str().unwrap_or("?");
                        let limit = a["daily_limit_secs"].as_u64().unwrap_or(0);
                        let used = item["used_today_secs"].as_u64().unwrap_or(0);
                        let remaining = item["remaining_secs"].as_u64().unwrap_or(0);
                        let exhausted = item["exhausted"].as_bool().unwrap_or(false);
                        let pct = if limit > 0 { used * 100 / limit } else { 0 };
                        let status = if exhausted { "[BLOCKED]" } else { "          " };
                        println!(
                            "  {status} {kind:<6} {target:<30} {:>3}m/{:>3}m  ({pct}%, {rem}m left)",
                            used / 60,
                            limit / 60,
                            rem = remaining / 60,
                        );
                    }
                }
            }
            Err(e) => eprintln!("Could not reach Focuser desktop app (is it running?): {e}"),
        },
    }

    Ok(())
}

/// Minimal HTTP GET that returns the response body. No external HTTP crate.
fn http_get(path: &str) -> std::io::Result<String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let mut stream = TcpStream::connect_timeout(
        &"127.0.0.1:17549".parse().unwrap(),
        Duration::from_millis(1500),
    )?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    Ok(body)
}

fn parse_id(s: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|e| anyhow::anyhow!("Invalid ID \"{s}\": {e}"))
}
