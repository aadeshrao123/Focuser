//! Main service loop — ties together the blocking engine, IPC, and platform blocker.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use focuser_app::{AppContext, Command, CommandError, CommandResult, SystemSync, execute};
use focuser_common::extension::BrowserType;
use focuser_common::ipc::*;
use focuser_core::BlockEngine;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};

use crate::ipc;
use crate::platform;

/// Tracks a connected browser extension.
#[allow(dead_code)]
pub(crate) struct ExtensionConnection {
    browser: BrowserType,
    extension_version: String,
    connected_at: Instant,
    last_seen: Instant,
}

/// Shared extension connection state.
pub(crate) type ExtensionConnections = Arc<Mutex<HashMap<BrowserType, ExtensionConnection>>>;

/// The service's implementation of the one side effect commands need.
struct HostsSync;

impl SystemSync for HostsSync {
    fn sync_hosts(&self, domains: &[String]) {
        if let Err(e) = crate::hosts::apply_blocks(domains) {
            error!(error = %e, "Failed to sync hosts file");
        }
    }
}

pub struct FocuserService {
    ctx: Arc<AppContext>,
    blocker: Arc<dyn focuser_common::platform::PlatformBlocker>,
    started_at: Instant,
    extension_connections: ExtensionConnections,
    enforcement: Arc<Mutex<crate::enforcement::BrowserEnforcement>>,
}

impl FocuserService {
    pub fn new(engine: BlockEngine) -> Result<Self> {
        let blocker: Arc<dyn focuser_common::platform::PlatformBlocker> =
            Arc::from(platform::create_blocker());

        let (grace_seconds, enforce_browsers) = enforcement_settings(engine.db());

        info!(
            grace_seconds,
            enforce_browsers, "Browser enforcement settings loaded"
        );

        Ok(Self {
            ctx: Arc::new(AppContext::new(engine, Arc::new(HostsSync))),
            blocker,
            started_at: Instant::now(),
            extension_connections: Arc::new(Mutex::new(HashMap::new())),
            enforcement: Arc::new(Mutex::new(crate::enforcement::BrowserEnforcement::new(
                grace_seconds,
                enforce_browsers,
            ))),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Focuser service running");

        // Apply initial blocks
        self.apply_website_blocks();

        // Clone references for IPC handler
        let ctx = Arc::clone(&self.ctx);
        let started_at = self.started_at;
        let ext_conns = Arc::clone(&self.extension_connections);
        let blocker_for_ipc = Arc::clone(&self.blocker);
        let enforcement_for_ipc = Arc::clone(&self.enforcement);

        // IPC handler
        let handler: ipc::RequestHandler = Box::new(move |request| {
            handle_request(
                &ctx,
                &started_at,
                &ext_conns,
                &blocker_for_ipc,
                &enforcement_for_ipc,
                request,
            )
        });

        // Spawn IPC server
        let ipc_handle = tokio::spawn(async move {
            if let Err(e) = ipc::serve(handler).await {
                error!(error = %e, "IPC server failed");
            }
        });

        // Spawn tick loop: engine refresh + browser enforcement + protection enforcement
        let ctx_for_tick = Arc::clone(&self.ctx);
        let ext_conns_for_tick = Arc::clone(&self.extension_connections);
        let blocker_for_tick = Arc::clone(&self.blocker);
        let enforcement_for_tick = Arc::clone(&self.enforcement);

        let tick_handle = tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(2));
            loop {
                tick.tick().await;

                // Refresh engine cache
                if let Ok(mut eng) = ctx_for_tick.engine.lock()
                    && let Err(e) = eng.refresh()
                {
                    warn!(error = %e, "Failed to refresh engine");
                }

                // Browser enforcement: detect browsers without extension
                let processes = match blocker_for_tick.list_running_processes() {
                    Ok(p) => p,
                    Err(e) => {
                        debug!(error = %e, "Failed to list processes for enforcement");
                        continue;
                    }
                };

                let connected: std::collections::HashSet<BrowserType> = {
                    let conns = ext_conns_for_tick.lock().unwrap();
                    conns.keys().cloned().collect()
                };

                // Re-read on every tick: changing these in the UI must take
                // effect without restarting the service.
                {
                    let eng = ctx_for_tick.engine.lock().unwrap();
                    let (grace, enabled) = enforcement_settings(eng.db());
                    enforcement_for_tick
                        .lock()
                        .unwrap()
                        .reconfigure(grace, enabled);
                }

                let has_active_blocks = {
                    let eng = ctx_for_tick.engine.lock().unwrap();
                    eng.block_lists().iter().any(|l| l.enabled)
                };

                let pids_to_kill = {
                    let mut enf = enforcement_for_tick.lock().unwrap();
                    enf.evaluate(&processes, &connected, has_active_blocks)
                };

                // Deduplicate by exe name — kill_blocked_app kills all matching processes
                let mut killed_names = std::collections::HashSet::new();
                for pid in pids_to_kill {
                    let name = processes
                        .iter()
                        .find(|p| p.pid == pid)
                        .map(|p| p.name.as_str())
                        .unwrap_or("unknown");

                    if killed_names.insert(name.to_string()) {
                        info!(name, "Terminating browser without Focuser extension");
                        let rule = focuser_common::types::AppRule::executable(name);
                        if let Err(e) = blocker_for_tick.kill_blocked_app(&rule) {
                            warn!(name, error = %e, "Failed to terminate browser");
                        }
                    }
                }
            }
        });

        // Wait for shutdown
        tokio::select! {
            _ = ipc_handle => {
                info!("IPC server stopped");
            }
            _ = tick_handle => {
                info!("Tick loop stopped");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down");
            }
        }

        // Cleanup: remove hosts file blocks
        info!("Cleaning up hosts file");
        if let Err(e) = self.blocker.unblock_all_websites() {
            error!(error = %e, "Failed to clean up hosts file");
        }

        Ok(())
    }

    fn apply_website_blocks(&self) {
        let engine = self.ctx.engine.lock().unwrap();
        let domains = engine.collect_blocked_domains();
        if domains.is_empty() {
            info!("No domains to block");
            return;
        }
        info!(count = domains.len(), "Applying website blocks");
        if let Err(e) = crate::hosts::apply_blocks(&domains) {
            error!(error = %e, "Failed to apply website blocks");
        }
    }
}

/// Translate an IPC request into a command, run it, and shape the reply.
///
/// Anything the command core can do goes through `execute` — this file used to
/// carry a second implementation of block-list editing, protection checks and
/// settings, which is exactly how a GUI and a service drift apart. What is left
/// here is genuinely service-shaped: uptime, extension connections, and the
/// capability probe.
fn handle_request(
    ctx: &Arc<AppContext>,
    started_at: &Instant,
    ext_conns: &ExtensionConnections,
    blocker: &Arc<dyn focuser_common::platform::PlatformBlocker>,
    enforcement: &Arc<Mutex<crate::enforcement::BrowserEnforcement>>,
    request: IpcRequest,
) -> IpcResponse {
    match request {
        IpcRequest::Ping => IpcResponse::Pong,

        // ─── Straight through to the command core ─────────────────
        IpcRequest::ListBlockLists => match execute(ctx, Command::ListBlockLists) {
            Ok(CommandResult::BlockLists(lists)) => IpcResponse::BlockLists(lists),
            other => unexpected(other),
        },

        IpcRequest::GetBlockList(id) => match execute(ctx, Command::ListBlockLists) {
            Ok(CommandResult::BlockLists(lists)) => match lists.into_iter().find(|l| l.id == id) {
                Some(list) => IpcResponse::BlockList(list),
                None => IpcResponse::Error(format!("not_found: no block list with id {id}")),
            },
            other => unexpected(other),
        },

        // The wire form carries a whole list, so it is created and then filled
        // in — both steps go through the core so protection and hosts sync apply.
        IpcRequest::CreateBlockList(list) => {
            match execute(
                ctx,
                Command::CreateBlockList {
                    name: list.name.clone(),
                },
            ) {
                Ok(CommandResult::BlockList(created)) => {
                    let mut filled = list;
                    filled.id = created.id;
                    ok_or_error(execute(
                        ctx,
                        Command::UpdateBlockList {
                            list: Box::new(filled),
                        },
                    ))
                }
                other => unexpected(other),
            }
        }

        IpcRequest::UpdateBlockList(list) => ok_or_error(execute(
            ctx,
            Command::UpdateBlockList {
                list: Box::new(list),
            },
        )),

        IpcRequest::DeleteBlockList(id) => {
            ok_or_error(execute(ctx, Command::DeleteBlockList { id }))
        }

        IpcRequest::SetBlockListEnabled { id, enabled } => {
            ok_or_error(execute(ctx, Command::ToggleBlockList { id, enabled }))
        }

        // Starting and stopping a block is enabling and disabling its list.
        IpcRequest::StartBlock { block_list_id, .. } => ok_or_error(execute(
            ctx,
            Command::ToggleBlockList {
                id: block_list_id,
                enabled: true,
            },
        )),

        IpcRequest::StopBlock { block_list_id } => ok_or_error(execute(
            ctx,
            Command::ToggleBlockList {
                id: block_list_id,
                enabled: false,
            },
        )),

        IpcRequest::CheckDomain(domain) => match execute(ctx, Command::CheckDomain { domain }) {
            Ok(CommandResult::Flag(blocked)) => IpcResponse::DomainBlocked(blocked),
            other => unexpected(other),
        },

        IpcRequest::GetStats { from, to } => match execute(ctx, Command::GetStats { from, to }) {
            Ok(CommandResult::Stats(stats)) => IpcResponse::Stats(stats),
            other => unexpected(other),
        },

        IpcRequest::GetSetting(key) => {
            match execute(ctx, Command::GetSetting { key, default: None }) {
                Ok(CommandResult::Setting(value)) => IpcResponse::Setting(value),
                other => unexpected(other),
            }
        }

        IpcRequest::SetSetting { key, value } => {
            ok_or_error(execute(ctx, Command::SetSetting { key, value }))
        }

        IpcRequest::EnableProtection {
            block_list_id,
            duration_minutes,
            prevent_uninstall,
            prevent_service_stop,
            prevent_modification,
        } => ok_or_error(execute(
            ctx,
            Command::EnableProtection {
                list_id: block_list_id,
                duration_minutes,
                prevent_uninstall,
                prevent_service_stop,
                prevent_modification,
            },
        )),

        IpcRequest::GetProtectionStatus => {
            let eng = ctx.engine.lock().unwrap();
            IpcResponse::ProtectionStatus(eng.active_protection_info())
        }

        // ─── Service-shaped: no command equivalent ────────────────
        IpcRequest::GetStatus => {
            let eng = ctx.engine.lock().unwrap();
            let active_blocks: Vec<ActiveBlockInfo> = eng
                .block_lists()
                .iter()
                .filter(|l| l.enabled)
                .map(|l| ActiveBlockInfo {
                    block_list_id: l.id,
                    block_list_name: l.name.clone(),
                    started_at: l.created_at,
                    expires_at: None,
                    blocked_websites: l.websites.len() as u32,
                    blocked_apps: l.applications.len() as u32,
                })
                .collect();

            IpcResponse::Status(ServiceStatus {
                running: true,
                active_blocks,
                total_blocked_today: eng.db().get_total_blocked_today().unwrap_or(0),
                uptime_seconds: started_at.elapsed().as_secs(),
            })
        }

        IpcRequest::CheckApp(app) => {
            let eng = ctx.engine.lock().unwrap();
            IpcResponse::AppBlocked(eng.check_app(&app, None, None).is_some())
        }

        IpcRequest::GetBlockedAttempts => {
            let eng = ctx.engine.lock().unwrap();
            match eng.db().get_total_blocked_today() {
                Ok(count) => IpcResponse::BlockedAttempts(count),
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::GetExtensionRules => {
            let eng = ctx.engine.lock().unwrap();
            IpcResponse::ExtensionRules(eng.compile_extension_rules())
        }

        IpcRequest::ExtensionEvent(event) => handle_extension_event(ctx, ext_conns, event),

        IpcRequest::GetCapabilities => {
            let hosts_ok = crate::hosts::is_domain_blocked("localhost").is_ok();
            let conns = ext_conns.lock().unwrap();
            let connected_browsers: Vec<BrowserType> = conns.keys().cloned().collect();

            IpcResponse::Capabilities(focuser_common::extension::BlockingCapabilities {
                hosts_file: hosts_ok,
                extension_connected: !connected_browsers.is_empty(),
                connected_browsers,
            })
        }

        // Richer than the core's `GetBrowserStatus`: it also reports how long a
        // browser has left before enforcement closes it, which only the service
        // knows.
        IpcRequest::GetBrowserStatus => {
            let processes = blocker.list_running_processes().unwrap_or_default();
            let conns = ext_conns.lock().unwrap();
            let enf = enforcement.lock().unwrap();

            let statuses = focuser_common::browser::KNOWN_BROWSERS
                .iter()
                .map(|info| {
                    let browser_type = &info.browser_type;
                    let is_running = processes.iter().any(|p| {
                        focuser_common::browser::identify_browser(&p.name)
                            .is_some_and(|b| b.browser_type == *browser_type)
                    });

                    focuser_common::browser::BrowserStatusInfo {
                        browser_type: browser_type.clone(),
                        display_name: info.display_name.to_string(),
                        is_running,
                        extension_connected: conns.contains_key(browser_type),
                        grace_period_remaining_secs: enf.grace_remaining(browser_type),
                    }
                })
                .collect();

            IpcResponse::BrowserStatus(statuses)
        }

        IpcRequest::Shutdown => {
            let eng = ctx.engine.lock().unwrap();
            if eng.has_service_protection() {
                return IpcResponse::Error(
                    "protected: cannot shut down the service until all protections expire"
                        .to_string(),
                );
            }
            drop(eng);
            info!("Shutdown requested via IPC");
            std::process::exit(0);
        }
    }
}

fn handle_extension_event(
    ctx: &Arc<AppContext>,
    ext_conns: &ExtensionConnections,
    event: focuser_common::extension::ExtensionEvent,
) -> IpcResponse {
    use focuser_common::extension::ExtensionEvent;

    match event {
        ExtensionEvent::Connected {
            browser,
            extension_version,
        } => {
            info!(browser = ?browser, version = %extension_version, "Browser extension connected");
            let now = Instant::now();
            ext_conns.lock().unwrap().insert(
                browser.clone(),
                ExtensionConnection {
                    browser,
                    extension_version,
                    connected_at: now,
                    last_seen: now,
                },
            );
            IpcResponse::Ok
        }

        ExtensionEvent::Disconnected { browser } => {
            info!(browser = ?browser, "Browser extension disconnected");
            ext_conns.lock().unwrap().remove(&browser);
            IpcResponse::Ok
        }

        ExtensionEvent::RequestRules => {
            let eng = ctx.engine.lock().unwrap();
            IpcResponse::ExtensionRules(eng.compile_extension_rules())
        }

        ExtensionEvent::Blocked { url, .. } => {
            // The extension reports a full URL; statistics are keyed by host.
            let domain = url
                .split("://")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or(&url);
            let eng = ctx.engine.lock().unwrap();
            let _ = eng.record_blocked(domain);
            IpcResponse::Ok
        }

        ExtensionEvent::UsageReport {
            domain, seconds, ..
        } => {
            debug!(domain = %domain, seconds, "Usage report from extension");
            IpcResponse::Ok
        }
    }
}

/// `Ok` for anything that succeeded; the error's stable code leads the message
/// so a caller can branch on it without parsing prose.
fn ok_or_error(result: Result<CommandResult, CommandError>) -> IpcResponse {
    match result {
        Ok(_) => IpcResponse::Ok,
        Err(e) => IpcResponse::Error(format!("{}: {e}", e.code())),
    }
}

/// A command returned a variant this request does not know how to shape. That
/// is a bug in the mapping above, not a caller error.
fn unexpected(result: Result<CommandResult, CommandError>) -> IpcResponse {
    match result {
        Ok(other) => IpcResponse::Error(format!("internal: unexpected result {other:?}")),
        Err(e) => IpcResponse::Error(format!("{}: {e}", e.code())),
    }
}

/// Grace period and whether unsupported browsers get closed at all.
///
/// Read on each tick rather than only at startup — the same stale-read bug the
/// GUI's blocking loop had.
fn enforcement_settings(db: &focuser_core::db::Database) -> (u64, bool) {
    let grace = db
        .get_setting_or_default("extension_grace_period", "60")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    let enabled = db
        .get_setting_or_default("block_unsupported_browsers", "true")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);

    (grace, enabled)
}
