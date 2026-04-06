//! Main service loop — ties together the blocking engine, IPC, and platform blocker.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
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

pub struct FocuserService {
    engine: Arc<Mutex<BlockEngine>>,
    blocker: Arc<dyn focuser_common::platform::PlatformBlocker>,
    started_at: Instant,
    extension_connections: ExtensionConnections,
    enforcement: Arc<Mutex<crate::enforcement::BrowserEnforcement>>,
}

impl FocuserService {
    pub fn new(engine: BlockEngine) -> Result<Self> {
        let blocker: Arc<dyn focuser_common::platform::PlatformBlocker> =
            Arc::from(platform::create_blocker());

        // Read enforcement settings
        let grace_seconds = engine
            .db()
            .get_setting_or_default("extension_grace_period", "60")
            .unwrap_or_else(|_| "60".to_string())
            .parse::<u64>()
            .unwrap_or(60);
        let enforce_browsers = engine
            .db()
            .get_setting_or_default("block_unsupported_browsers", "true")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        info!(
            grace_seconds,
            enforce_browsers, "Browser enforcement settings loaded"
        );

        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
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
        let engine = Arc::clone(&self.engine);
        let started_at = self.started_at;
        let ext_conns = Arc::clone(&self.extension_connections);
        let blocker_for_ipc = Arc::clone(&self.blocker);
        let enforcement_for_ipc = Arc::clone(&self.enforcement);

        // IPC handler
        let handler: ipc::RequestHandler = Box::new(move |request| {
            handle_request(
                &engine,
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
        let engine_for_tick = Arc::clone(&self.engine);
        let ext_conns_for_tick = Arc::clone(&self.extension_connections);
        let blocker_for_tick = Arc::clone(&self.blocker);
        let enforcement_for_tick = Arc::clone(&self.enforcement);

        let tick_handle = tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(2));
            loop {
                tick.tick().await;

                // Refresh engine cache
                if let Ok(mut eng) = engine_for_tick.lock()
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

                let has_active_blocks = {
                    let eng = engine_for_tick.lock().unwrap();
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
        let engine = self.engine.lock().unwrap();
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

fn handle_request(
    engine: &Arc<Mutex<BlockEngine>>,
    started_at: &Instant,
    ext_conns: &ExtensionConnections,
    blocker: &Arc<dyn focuser_common::platform::PlatformBlocker>,
    enforcement: &Arc<Mutex<crate::enforcement::BrowserEnforcement>>,
    request: IpcRequest,
) -> IpcResponse {
    match request {
        IpcRequest::Ping => IpcResponse::Pong,

        IpcRequest::GetStatus => {
            let eng = engine.lock().unwrap();
            let lists = eng.block_lists();
            let active_blocks: Vec<ActiveBlockInfo> = lists
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

            let total_blocked_today = eng.db().get_total_blocked_today().unwrap_or(0);

            IpcResponse::Status(ServiceStatus {
                running: true,
                active_blocks,
                total_blocked_today,
                uptime_seconds: started_at.elapsed().as_secs(),
            })
        }

        IpcRequest::ListBlockLists => {
            let eng = engine.lock().unwrap();
            IpcResponse::BlockLists(eng.block_lists().to_vec())
        }

        IpcRequest::GetBlockList(id) => {
            let eng = engine.lock().unwrap();
            match eng.db().get_block_list(id) {
                Ok(list) => IpcResponse::BlockList(list),
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::CreateBlockList(list) => {
            let mut eng = engine.lock().unwrap();
            match eng.db().create_block_list(&list) {
                Ok(()) => {
                    let _ = eng.refresh();
                    IpcResponse::Ok
                }
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::UpdateBlockList(list) => {
            let mut eng = engine.lock().unwrap();
            if eng.is_block_list_protected(list.id) {
                return IpcResponse::Error(
                    "Protection is active — cannot modify this block list until it expires"
                        .to_string(),
                );
            }
            match eng.db().update_block_list(&list) {
                Ok(()) => {
                    let _ = eng.refresh();
                    IpcResponse::Ok
                }
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::DeleteBlockList(id) => {
            let mut eng = engine.lock().unwrap();
            if eng.is_block_list_protected(id) {
                return IpcResponse::Error(
                    "Protection is active �� cannot delete this block list until it expires"
                        .to_string(),
                );
            }
            match eng.db().delete_block_list(id) {
                Ok(()) => {
                    let _ = eng.refresh();
                    IpcResponse::Ok
                }
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::SetBlockListEnabled { id, enabled } => {
            let mut eng = engine.lock().unwrap();
            if !enabled && eng.is_block_list_protected(id) {
                return IpcResponse::Error(
                    "Protection is active — cannot disable this block list until it expires"
                        .to_string(),
                );
            }
            match eng.db().get_block_list(id) {
                Ok(mut list) => {
                    list.enabled = enabled;
                    list.updated_at = chrono::Utc::now();
                    match eng.db().update_block_list(&list) {
                        Ok(()) => {
                            let _ = eng.refresh();
                            IpcResponse::Ok
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::CheckDomain(domain) => {
            let eng = engine.lock().unwrap();
            let blocked = eng.check_domain(&domain).is_some();
            IpcResponse::DomainBlocked(blocked)
        }

        IpcRequest::CheckApp(app) => {
            let eng = engine.lock().unwrap();
            let blocked = eng.check_app(&app, None, None).is_some();
            IpcResponse::AppBlocked(blocked)
        }

        IpcRequest::GetStats { from, to } => {
            let eng = engine.lock().unwrap();
            match eng.db().get_stats(from, to) {
                Ok(stats) => IpcResponse::Stats(stats),
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::GetSetting(key) => {
            let eng = engine.lock().unwrap();
            match eng.db().get_setting(&key) {
                Ok(value) => IpcResponse::Setting(value),
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::SetSetting { key, value } => {
            let eng = engine.lock().unwrap();
            match eng.db().set_setting(&key, &value) {
                Ok(()) => IpcResponse::Ok,
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::GetBlockedAttempts => {
            let eng = engine.lock().unwrap();
            match eng.db().get_total_blocked_today() {
                Ok(count) => IpcResponse::BlockedAttempts(count),
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::StartBlock { block_list_id, .. } => {
            let mut eng = engine.lock().unwrap();
            match eng.db().get_block_list(block_list_id) {
                Ok(mut list) => {
                    list.enabled = true;
                    list.updated_at = chrono::Utc::now();
                    match eng.db().update_block_list(&list) {
                        Ok(()) => {
                            let _ = eng.refresh();
                            IpcResponse::Ok
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::StopBlock { block_list_id } => {
            let mut eng = engine.lock().unwrap();
            if eng.is_block_list_protected(block_list_id) {
                return IpcResponse::Error(
                    "Protection is active — cannot stop this block until it expires".to_string(),
                );
            }
            match eng.db().get_block_list(block_list_id) {
                Ok(mut list) => {
                    // Check if there's an active lock
                    if list.lock.is_some() {
                        return IpcResponse::Error(
                            "Cannot stop block — a lock is active".to_string(),
                        );
                    }
                    list.enabled = false;
                    list.updated_at = chrono::Utc::now();
                    match eng.db().update_block_list(&list) {
                        Ok(()) => {
                            let _ = eng.refresh();
                            IpcResponse::Ok
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::GetExtensionRules => {
            let eng = engine.lock().unwrap();
            let rules = eng.compile_extension_rules();
            IpcResponse::ExtensionRules(rules)
        }

        IpcRequest::ExtensionEvent(event) => {
            info!(event = ?event, "Extension event received");
            match event {
                focuser_common::extension::ExtensionEvent::Connected {
                    browser,
                    extension_version,
                } => {
                    info!(
                        browser = ?browser,
                        version = %extension_version,
                        "Browser extension connected"
                    );
                    let now = Instant::now();
                    let mut conns = ext_conns.lock().unwrap();
                    conns.insert(
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
                focuser_common::extension::ExtensionEvent::Disconnected { browser } => {
                    info!(browser = ?browser, "Browser extension disconnected");
                    let mut conns = ext_conns.lock().unwrap();
                    conns.remove(&browser);
                    IpcResponse::Ok
                }
                focuser_common::extension::ExtensionEvent::RequestRules => {
                    let eng = engine.lock().unwrap();
                    let rules = eng.compile_extension_rules();
                    IpcResponse::ExtensionRules(rules)
                }
                focuser_common::extension::ExtensionEvent::Blocked { url, .. } => {
                    // Extract domain from URL for stats
                    let domain = url
                        .split("://")
                        .nth(1)
                        .and_then(|s| s.split('/').next())
                        .unwrap_or(&url);
                    let eng = engine.lock().unwrap();
                    let _ = eng.record_blocked(domain);
                    IpcResponse::Ok
                }
                focuser_common::extension::ExtensionEvent::UsageReport {
                    domain, seconds, ..
                } => {
                    debug!(domain = %domain, seconds, "Usage report from extension");
                    // TODO: store usage duration in stats table
                    IpcResponse::Ok
                }
            }
        }

        IpcRequest::GetCapabilities => {
            let hosts_ok = crate::hosts::is_domain_blocked("localhost").is_ok();
            let conns = ext_conns.lock().unwrap();
            let connected_browsers: Vec<BrowserType> = conns.keys().cloned().collect();
            let caps = focuser_common::extension::BlockingCapabilities {
                hosts_file: hosts_ok,
                extension_connected: !connected_browsers.is_empty(),
                connected_browsers,
            };
            IpcResponse::Capabilities(caps)
        }

        IpcRequest::GetBrowserStatus => {
            let processes = blocker.list_running_processes().unwrap_or_default();
            let conns = ext_conns.lock().unwrap();
            let enf = enforcement.lock().unwrap();

            // Collect status for all known browsers
            let mut statuses: Vec<focuser_common::browser::BrowserStatusInfo> = Vec::new();
            let mut seen = std::collections::HashSet::new();

            for browser_info in focuser_common::browser::KNOWN_BROWSERS {
                let bt = &browser_info.browser_type;
                if !seen.insert(bt.clone()) {
                    continue;
                }

                let is_running = processes.iter().any(|p| {
                    focuser_common::browser::identify_browser(&p.name)
                        .is_some_and(|b| b.browser_type == *bt)
                });

                let extension_connected = conns.contains_key(bt);
                let grace_remaining = enf.grace_remaining(bt);

                statuses.push(focuser_common::browser::BrowserStatusInfo {
                    browser_type: bt.clone(),
                    display_name: browser_info.display_name.to_string(),
                    is_running,
                    extension_connected,
                    grace_period_remaining_secs: grace_remaining,
                });
            }

            IpcResponse::BrowserStatus(statuses)
        }

        IpcRequest::EnableProtection {
            block_list_id,
            duration_minutes,
            prevent_uninstall,
            prevent_service_stop,
            prevent_modification,
        } => {
            let mut eng = engine.lock().unwrap();
            match eng.db().get_block_list(block_list_id) {
                Ok(mut list) => {
                    if list.is_modification_protected() {
                        return IpcResponse::Error(
                            "Protection is already active on this block list".to_string(),
                        );
                    }

                    let now = chrono::Utc::now();
                    list.protection = Some(focuser_common::types::Protection {
                        prevent_uninstall,
                        prevent_service_stop,
                        prevent_modification,
                        started_at: now,
                        expires_at: now + chrono::Duration::minutes(duration_minutes as i64),
                    });
                    list.updated_at = now;

                    list.enabled = true;

                    match eng.db().update_block_list(&list) {
                        Ok(()) => {
                            let _ = eng.refresh();
                            info!(
                                block_list = %list.name,
                                duration_minutes,
                                "Protection enabled"
                            );
                            IpcResponse::Ok
                        }
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }
                Err(e) => IpcResponse::Error(e.to_string()),
            }
        }

        IpcRequest::GetProtectionStatus => {
            let eng = engine.lock().unwrap();
            IpcResponse::ProtectionStatus(eng.active_protection_info())
        }

        IpcRequest::Shutdown => {
            let eng = engine.lock().unwrap();
            if eng.has_service_protection() {
                return IpcResponse::Error(
                    "Protection is active — cannot shut down the service until all protections expire"
                        .to_string(),
                );
            }
            drop(eng);
            info!("Shutdown requested via IPC");
            std::process::exit(0);
        }
    }
}
