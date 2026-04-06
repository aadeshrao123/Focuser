//! IPC message types for communication between CLI/GUI and the service.

use serde::{Deserialize, Serialize};

use crate::browser::BrowserStatusInfo;
use crate::extension::{BlockingCapabilities, ExtensionEvent, ExtensionRuleSet};
use crate::types::{BlockList, EntityId, UsageStat};

/// Request from client (CLI/GUI) to service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    // ─── Block list management ──────────────────────────────
    /// Create a new block list.
    CreateBlockList(BlockList),
    /// Update an existing block list.
    UpdateBlockList(BlockList),
    /// Delete a block list by ID.
    DeleteBlockList(EntityId),
    /// Get a block list by ID.
    GetBlockList(EntityId),
    /// List all block lists.
    ListBlockLists,
    /// Enable or disable a block list.
    SetBlockListEnabled {
        id: EntityId,
        enabled: bool,
    },

    // ─── Blocking control ───────────────────────────────────
    /// Start a block with an optional timer (minutes).
    StartBlock {
        block_list_id: EntityId,
        duration_minutes: Option<u32>,
    },
    /// Stop a block (only works if no lock prevents it).
    StopBlock {
        block_list_id: EntityId,
    },

    // ─── Status ─────────────────────────────────────────────
    /// Get current blocking status.
    GetStatus,
    /// Check if a specific domain is blocked.
    CheckDomain(String),
    /// Check if a specific app is blocked.
    CheckApp(String),

    // ─── Statistics ─────────────────────────────────────────
    /// Get usage statistics for a date range.
    GetStats {
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    },
    /// Get blocked attempt count.
    GetBlockedAttempts,

    // ─── Settings ────────────────────────────────────────────
    /// Get a setting value by key.
    GetSetting(String),
    /// Set a setting value.
    SetSetting {
        key: String,
        value: String,
    },

    // ─── Extension ──────────────────────────────────���────────
    /// Get the compiled rule set for the browser extension.
    GetExtensionRules,
    /// Report an event from the browser extension.
    ExtensionEvent(ExtensionEvent),
    /// Get current blocking capabilities (what's available).
    GetCapabilities,
    /// Get status of all detected browsers and their extension state.
    GetBrowserStatus,

    // ─── Protection ──────────────────────────────────────────
    EnableProtection {
        block_list_id: EntityId,
        duration_minutes: u32,
        prevent_uninstall: bool,
        prevent_service_stop: bool,
        prevent_modification: bool,
    },
    GetProtectionStatus,

    // ─── Service control ────────────────────────────────────
    /// Ping — check if service is alive.
    Ping,
    /// Graceful shutdown.
    Shutdown,
}

/// Response from service to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    /// Operation succeeded with no data.
    Ok,
    /// Operation failed.
    Error(String),
    /// Single block list.
    BlockList(BlockList),
    /// Multiple block lists.
    BlockLists(Vec<BlockList>),
    /// Current status of the service.
    Status(ServiceStatus),
    /// Domain check result.
    DomainBlocked(bool),
    /// App check result.
    AppBlocked(bool),
    /// Usage statistics.
    Stats(Vec<UsageStat>),
    /// Blocked attempt count.
    BlockedAttempts(u64),
    /// Setting value (None if not set).
    Setting(Option<String>),
    /// Compiled rules for the browser extension.
    ExtensionRules(ExtensionRuleSet),
    /// Current blocking capabilities.
    Capabilities(BlockingCapabilities),
    /// Browser detection and extension status.
    BrowserStatus(Vec<BrowserStatusInfo>),
    ProtectionStatus(Vec<ProtectionInfo>),
    /// Pong response.
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectionInfo {
    pub block_list_id: EntityId,
    pub block_list_name: String,
    pub prevent_uninstall: bool,
    pub prevent_service_stop: bool,
    pub prevent_modification: bool,
    pub remaining_seconds: u64,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Overall service status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub running: bool,
    pub active_blocks: Vec<ActiveBlockInfo>,
    pub total_blocked_today: u64,
    pub uptime_seconds: u64,
}

/// Info about a currently active block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBlockInfo {
    pub block_list_id: EntityId,
    pub block_list_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub blocked_websites: u32,
    pub blocked_apps: u32,
}
