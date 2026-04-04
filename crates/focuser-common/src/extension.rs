//! Browser extension protocol types.
//!
//! This module defines the message format between the Focuser service and
//! a future browser extension (Chrome/Firefox/Edge via Native Messaging).
//!
//! The extension handles blocking capabilities that the hosts file cannot:
//! - Keyword-in-URL blocking (e.g., block any URL containing "game")
//! - URL path blocking (e.g., block reddit.com/r/gaming but allow reddit.com/r/rust)
//! - Wildcard patterns on full URLs
//! - Custom block page display
//! - Active usage time tracking (how long a tab is focused)
//!
//! The service pushes blocking rules to the extension whenever they change.
//! The extension reports back blocked attempts and usage time.
//!
//! ## Architecture
//!
//! ```text
//!                    ┌──────────────┐
//!                    │   Browser     │
//!                    │  Extension    │
//!                    │  (content +   │
//!                    │   background) │
//!                    └──────┬───────┘
//!                           │ Native Messaging (stdin/stdout JSON)
//!                    ┌──────┴───────┐
//!                    │  Native Host │  ← Small binary that bridges to service
//!                    │  (focuser-   │
//!                    │   native)    │
//!                    └──────┬───────┘
//!                           │ IPC (TCP / Unix socket)
//!                    ┌──────┴───────┐
//!                    │   Focuser    │
//!                    │   Service    │
//!                    └──────────────┘
//! ```
//!
//! Native Messaging uses length-prefixed JSON on stdin/stdout.
//! The native host binary translates these to normal IPC requests.

use serde::{Deserialize, Serialize};

// ─── Messages from Service → Extension ──────────────────────────────

/// A compiled set of rules pushed to the extension.
///
/// The service sends this whenever block lists change. The extension
/// caches it and uses it for real-time URL matching without round-trips.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionRuleSet {
    /// Version counter — extension discards stale updates.
    pub version: u64,
    /// Domains to block (exact match — also handled by hosts file as backup).
    pub blocked_domains: Vec<String>,
    /// Keywords to match anywhere in the URL.
    pub blocked_keywords: Vec<String>,
    /// Wildcard patterns to match against full URLs.
    pub blocked_wildcards: Vec<String>,
    /// URL path prefixes to block (e.g., "reddit.com/r/gaming").
    pub blocked_url_paths: Vec<String>,
    /// If true, block all URLs except those in `allowed_domains`.
    pub block_entire_internet: bool,
    /// Domains allowed even when everything else is blocked (exceptions).
    pub allowed_domains: Vec<String>,
    /// Wildcard patterns for exceptions.
    pub allowed_wildcards: Vec<String>,
}

impl ExtensionRuleSet {
    pub fn empty() -> Self {
        Self {
            version: 0,
            blocked_domains: Vec::new(),
            blocked_keywords: Vec::new(),
            blocked_wildcards: Vec::new(),
            blocked_url_paths: Vec::new(),
            block_entire_internet: false,
            allowed_domains: Vec::new(),
            allowed_wildcards: Vec::new(),
        }
    }

    /// Returns true if this rule set has any rules that require the extension
    /// (i.e., cannot be handled by hosts file alone).
    pub fn requires_extension(&self) -> bool {
        !self.blocked_keywords.is_empty()
            || !self.blocked_wildcards.is_empty()
            || !self.blocked_url_paths.is_empty()
            || self.block_entire_internet
    }
}

// ─── Messages from Extension → Service ──────────────────────────────

/// Event reported by the extension back to the service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtensionEvent {
    /// Extension connected and is ready.
    Connected {
        browser: BrowserType,
        extension_version: String,
    },
    /// Extension blocked a navigation.
    Blocked {
        url: String,
        /// Which rule triggered the block (for stats).
        matched_rule: MatchedRule,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// Tab usage tracking — how long a domain was in the focused tab.
    UsageReport {
        domain: String,
        seconds: u64,
        date: chrono::NaiveDate,
    },
    /// Extension is requesting the current rule set (e.g., on startup).
    RequestRules,
    /// Extension is disconnecting.
    Disconnected { browser: BrowserType },
}

/// Which browser the extension is running in.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BrowserType {
    Chrome,
    Firefox,
    Edge,
    Brave,
    Opera,
    Other(String),
}

/// Describes which rule matched (for stats/debugging).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchedRule {
    Domain(String),
    Keyword(String),
    Wildcard(String),
    UrlPath(String),
    EntireInternet,
}

// ─── Native Messaging Wire Format ───────────────────────────────────

/// Message envelope for Native Messaging protocol.
/// The native host reads/writes these as length-prefixed JSON on stdin/stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeMessage {
    /// Message type tag for quick dispatch.
    pub msg_type: NativeMessageType,
    /// JSON payload — the actual ExtensionRuleSet, ExtensionEvent, etc.
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NativeMessageType {
    /// Service → Extension: here are your blocking rules.
    RuleUpdate,
    /// Extension → Service: something happened (blocked, usage, etc.).
    Event,
    /// Either direction: are you there?
    Ping,
    /// Either direction: yes I'm here.
    Pong,
    /// Service → Extension: error message.
    Error,
}

// ─── Extension Capability Flags ─────────────────────────────────────

/// What blocking capabilities are currently available.
///
/// The service tracks this to warn the user when they create rules
/// that need the extension but the extension isn't connected.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockingCapabilities {
    /// Hosts file is writable — domain blocking works.
    pub hosts_file: bool,
    /// At least one browser extension is connected.
    pub extension_connected: bool,
    /// Connected browsers.
    pub connected_browsers: Vec<BrowserType>,
}

impl BlockingCapabilities {
    /// Check if keyword/wildcard/URL-path blocking is available.
    pub fn supports_advanced_web_rules(&self) -> bool {
        self.extension_connected
    }

    /// Human-readable summary of what's missing.
    pub fn missing_capabilities(&self) -> Vec<String> {
        let mut missing = Vec::new();
        if !self.hosts_file {
            missing.push("Hosts file not writable — run service as admin/root".into());
        }
        if !self.extension_connected {
            missing.push(
                "No browser extension connected — keyword, wildcard, and URL path \
                 blocking requires the Focuser browser extension"
                    .into(),
            );
        }
        missing
    }
}
