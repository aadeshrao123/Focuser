use focuser_common::error::Result;
use focuser_common::extension::ExtensionRuleSet;
use focuser_common::platform::RunningProcess;
use focuser_common::types::{BlockList, ExceptionType, WebsiteMatchType};
use tracing::{debug, info, warn};

use crate::db::Database;

/// The blocking engine evaluates rules against domains and processes.
///
/// It loads block lists from the database and determines what should be blocked.
pub struct BlockEngine {
    db: Database,
    /// Cached block lists for fast evaluation (refreshed periodically).
    cached_lists: Vec<BlockList>,
}

impl BlockEngine {
    pub fn new(db: Database) -> Result<Self> {
        let cached_lists = db.list_block_lists()?;
        info!(count = cached_lists.len(), "Block engine initialized");
        Ok(Self { db, cached_lists })
    }

    /// Reload block lists from the database.
    pub fn refresh(&mut self) -> Result<()> {
        self.cached_lists = self.db.list_block_lists()?;
        debug!(count = self.cached_lists.len(), "Refreshed block lists");
        Ok(())
    }

    /// Check if a domain should be blocked.
    /// Returns the name of the first matching block list, or None.
    pub fn check_domain(&self, domain: &str) -> Option<&str> {
        for list in &self.cached_lists {
            if list.should_block_domain(domain) {
                return Some(&list.name);
            }
        }
        None
    }

    /// Check if an app should be blocked.
    /// Returns the name of the first matching block list, or None.
    pub fn check_app(
        &self,
        process_name: &str,
        exe_path: Option<&str>,
        window_title: Option<&str>,
    ) -> Option<&str> {
        for list in &self.cached_lists {
            if list.should_block_app(process_name, exe_path, window_title) {
                return Some(&list.name);
            }
        }
        None
    }

    /// Evaluate all running processes and return which ones should be blocked.
    pub fn find_blocked_processes<'a>(
        &'a self,
        processes: &'a [RunningProcess],
    ) -> Vec<(&'a RunningProcess, &'a str)> {
        let mut blocked = Vec::new();
        for proc in processes {
            if let Some(list_name) = self.check_app(
                &proc.name,
                proc.exe_path.as_deref(),
                proc.window_title.as_deref(),
            ) {
                blocked.push((proc, list_name));
            }
        }
        blocked
    }

    /// Collect all domains that need to be blocked (for hosts file generation).
    pub fn collect_blocked_domains(&self) -> Vec<String> {
        let mut domains = Vec::new();
        for list in &self.cached_lists {
            if !list.enabled {
                continue;
            }
            for rule in &list.websites {
                if !rule.enabled {
                    continue;
                }
                match &rule.match_type {
                    focuser_common::types::WebsiteMatchType::Domain(d) => {
                        domains.push(d.clone());
                        // Also add www. variant if not already present
                        if !d.starts_with("www.") {
                            domains.push(format!("www.{d}"));
                        }
                    }
                    _ => {
                        // Wildcard, keyword, URL path, and entire internet
                        // can't be represented in hosts file alone.
                        // These need DNS proxy or browser extension support.
                        warn!(
                            rule = ?rule.match_type,
                            "Rule type requires DNS proxy — hosts file only supports exact domains"
                        );
                    }
                }
            }
        }
        domains.sort();
        domains.dedup();
        domains
    }

    /// Compile all active rules into an `ExtensionRuleSet` that can be pushed
    /// to a connected browser extension.
    ///
    /// This separates rules by type: domains go to both hosts file AND extension,
    /// while keywords/wildcards/URL paths are extension-only.
    pub fn compile_extension_rules(&self) -> ExtensionRuleSet {
        let mut rules = ExtensionRuleSet::empty();
        rules.version = chrono::Utc::now().timestamp() as u64;

        for list in &self.cached_lists {
            if !list.enabled {
                continue;
            }

            // Compile website rules by type
            for rule in &list.websites {
                if !rule.enabled {
                    continue;
                }
                match &rule.match_type {
                    WebsiteMatchType::Domain(d) => {
                        rules.blocked_domains.push(d.clone());
                        if !d.starts_with("www.") {
                            rules.blocked_domains.push(format!("www.{d}"));
                        }
                    }
                    WebsiteMatchType::Keyword(kw) => {
                        rules.blocked_keywords.push(kw.clone());
                    }
                    WebsiteMatchType::Wildcard(pat) => {
                        rules.blocked_wildcards.push(pat.clone());
                    }
                    WebsiteMatchType::UrlPath(path) => {
                        rules.blocked_url_paths.push(path.clone());
                    }
                    WebsiteMatchType::EntireInternet => {
                        rules.block_entire_internet = true;
                    }
                }
            }

            // Compile exceptions
            for exc in &list.exceptions {
                if !exc.enabled {
                    continue;
                }
                match &exc.exception_type {
                    ExceptionType::Domain(d) => {
                        rules.allowed_domains.push(d.clone());
                    }
                    ExceptionType::Wildcard(pat) => {
                        rules.allowed_wildcards.push(pat.clone());
                    }
                    ExceptionType::LocalFiles => {
                        // Extension handles this natively via URL scheme check
                    }
                }
            }
        }

        rules.blocked_domains.sort();
        rules.blocked_domains.dedup();
        rules.allowed_domains.sort();
        rules.allowed_domains.dedup();

        rules
    }

    /// Check if any active rules require the browser extension to be enforced.
    pub fn has_extension_only_rules(&self) -> bool {
        self.compile_extension_rules().requires_extension()
    }

    /// Record a blocked attempt in the database.
    pub fn record_blocked(&self, domain_or_app: &str) -> Result<()> {
        self.db.record_blocked_attempt(domain_or_app)
    }

    /// Get a reference to the database.
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Get cached block lists.
    pub fn block_lists(&self) -> &[BlockList] {
        &self.cached_lists
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focuser_common::types::{AppRule, BlockList, WebsiteRule};

    fn setup_engine() -> BlockEngine {
        let db = Database::open_in_memory().unwrap();

        let mut social = BlockList::new("Social Media");
        social.websites.push(WebsiteRule::domain("reddit.com"));
        social.websites.push(WebsiteRule::domain("twitter.com"));
        social.websites.push(WebsiteRule::domain("facebook.com"));
        db.create_block_list(&social).unwrap();

        let mut games = BlockList::new("Games");
        games.applications.push(AppRule::executable("steam.exe"));
        games
            .websites
            .push(WebsiteRule::domain("store.steampowered.com"));
        db.create_block_list(&games).unwrap();

        BlockEngine::new(db).unwrap()
    }

    #[test]
    fn test_check_domain() {
        let engine = setup_engine();
        assert_eq!(engine.check_domain("reddit.com"), Some("Social Media"));
        assert_eq!(engine.check_domain("www.reddit.com"), Some("Social Media"));
        assert_eq!(engine.check_domain("example.com"), None);
    }

    #[test]
    fn test_check_app() {
        let engine = setup_engine();
        assert_eq!(engine.check_app("steam.exe", None, None), Some("Games"));
        assert_eq!(engine.check_app("chrome.exe", None, None), None);
    }

    #[test]
    fn test_collect_blocked_domains() {
        let engine = setup_engine();
        let domains = engine.collect_blocked_domains();
        assert!(domains.contains(&"reddit.com".to_string()));
        assert!(domains.contains(&"www.reddit.com".to_string()));
        assert!(domains.contains(&"twitter.com".to_string()));
        assert!(domains.contains(&"store.steampowered.com".to_string()));
    }

    #[test]
    fn test_extension_rules_domain_only() {
        let engine = setup_engine();
        let rules = engine.compile_extension_rules();

        // Domain-only rules don't require the extension
        assert!(!rules.requires_extension());
        assert!(rules.blocked_domains.contains(&"reddit.com".to_string()));
        assert!(rules.blocked_keywords.is_empty());
        assert!(rules.blocked_wildcards.is_empty());
        assert!(!rules.block_entire_internet);
    }

    #[test]
    fn test_extension_rules_with_keywords() {
        let db = Database::open_in_memory().unwrap();

        let mut list = BlockList::new("Mixed");
        list.websites.push(WebsiteRule::domain("reddit.com"));
        list.websites.push(WebsiteRule::keyword("gambling"));
        list.websites.push(WebsiteRule::wildcard("*.tiktok.*"));
        list.websites
            .push(WebsiteRule::url_path("youtube.com/shorts"));
        db.create_block_list(&list).unwrap();

        let engine = BlockEngine::new(db).unwrap();
        let rules = engine.compile_extension_rules();

        assert!(rules.requires_extension());
        assert!(rules.blocked_domains.contains(&"reddit.com".to_string()));
        assert!(rules.blocked_keywords.contains(&"gambling".to_string()));
        assert!(rules.blocked_wildcards.contains(&"*.tiktok.*".to_string()));
        assert!(
            rules
                .blocked_url_paths
                .contains(&"youtube.com/shorts".to_string())
        );
    }

    #[test]
    fn test_extension_rules_entire_internet_with_exceptions() {
        let db = Database::open_in_memory().unwrap();

        let mut list = BlockList::new("Nuclear");
        list.websites.push(WebsiteRule::entire_internet());
        list.exceptions
            .push(focuser_common::types::ExceptionRule::domain("github.com"));
        db.create_block_list(&list).unwrap();

        let engine = BlockEngine::new(db).unwrap();
        let rules = engine.compile_extension_rules();

        assert!(rules.requires_extension());
        assert!(rules.block_entire_internet);
        assert!(rules.allowed_domains.contains(&"github.com".to_string()));
    }

    #[test]
    fn test_has_extension_only_rules() {
        // Domain-only engine
        let engine = setup_engine();
        assert!(!engine.has_extension_only_rules());

        // Engine with keyword rule
        let db = Database::open_in_memory().unwrap();
        let mut list = BlockList::new("Keywords");
        list.websites.push(WebsiteRule::keyword("game"));
        db.create_block_list(&list).unwrap();
        let engine = BlockEngine::new(db).unwrap();
        assert!(engine.has_extension_only_rules());
    }
}
