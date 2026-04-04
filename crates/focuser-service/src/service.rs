//! Main service loop — ties together the blocking engine, IPC, and platform blocker.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use focuser_common::ipc::*;
use focuser_core::BlockEngine;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};

use crate::ipc;
use crate::platform;

pub struct FocuserService {
    engine: Arc<Mutex<BlockEngine>>,
    blocker: Box<dyn focuser_common::platform::PlatformBlocker>,
    started_at: Instant,
}

impl FocuserService {
    pub fn new(engine: BlockEngine) -> Result<Self> {
        let blocker = platform::create_blocker();
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            blocker,
            started_at: Instant::now(),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Focuser service running");

        // Apply initial blocks
        self.apply_website_blocks();

        // Clone engine for IPC handler
        let engine = Arc::clone(&self.engine);
        let started_at = self.started_at;

        // IPC handler
        let handler: ipc::RequestHandler =
            Box::new(move |request| handle_request(&engine, &started_at, request));

        // Spawn IPC server
        let ipc_handle = tokio::spawn(async move {
            if let Err(e) = ipc::serve(handler).await {
                error!(error = %e, "IPC server failed");
            }
        });

        // Spawn app blocker tick loop
        let engine_for_tick = Arc::clone(&self.engine);
        let tick_handle = tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(2));
            loop {
                tick.tick().await;
                // In a full implementation, this would check running processes
                // and kill blocked ones. For now, just refresh the engine cache.
                if let Ok(mut eng) = engine_for_tick.lock()
                    && let Err(e) = eng.refresh()
                {
                    warn!(error = %e, "Failed to refresh engine");
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
                focuser_common::extension::ExtensionEvent::Blocked { url, .. } => {
                    // Extract domain from URL for stats
                    let domain = url
                        .split("://")
                        .nth(1)
                        .and_then(|s| s.split('/').next())
                        .unwrap_or(&url);
                    let eng = engine.lock().unwrap();
                    let _ = eng.record_blocked(domain);
                }
                focuser_common::extension::ExtensionEvent::UsageReport {
                    domain, seconds, ..
                } => {
                    debug!(domain = %domain, seconds, "Usage report from extension");
                    // TODO: store usage duration in stats table
                }
                _ => {}
            }
            IpcResponse::Ok
        }

        IpcRequest::GetCapabilities => {
            let hosts_ok = crate::hosts::is_domain_blocked("localhost").is_ok();
            // TODO: track connected extensions in service state
            let caps = focuser_common::extension::BlockingCapabilities {
                hosts_file: hosts_ok,
                extension_connected: false,
                connected_browsers: vec![],
            };
            IpcResponse::Capabilities(caps)
        }

        IpcRequest::Shutdown => {
            info!("Shutdown requested via IPC");
            std::process::exit(0);
        }
    }
}
