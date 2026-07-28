use crate::host::{canonical_host, host_matches, wildcard_matches};
use crate::types::{
    AppMatchType, AppRule, BlockList, ExceptionType, WebsiteMatchType, WebsiteRule,
};

impl BlockList {
    /// Check if a domain should be blocked by this block list.
    /// Returns `true` if any website rule matches AND no exception overrides it.
    pub fn should_block_domain(&self, domain: &str) -> bool {
        if !self.is_effectively_active() {
            return false;
        }

        let domain_lower = domain.to_lowercase();

        // Check exceptions first
        if self.is_excepted_domain(&domain_lower) {
            return false;
        }

        // Check each website rule
        self.websites
            .iter()
            .any(|rule| rule.enabled && rule.matches_domain(&domain_lower))
    }

    /// Check if an application should be blocked.
    ///
    /// Honors the schedule: a list whose schedule is set but not currently
    /// active will not block any app, even if `enabled` is true.
    pub fn should_block_app(
        &self,
        process_name: &str,
        exe_path: Option<&str>,
        window_title: Option<&str>,
    ) -> bool {
        if !self.is_effectively_active() {
            return false;
        }

        self.applications
            .iter()
            .any(|rule| rule.enabled && rule.matches_process(process_name, exe_path, window_title))
    }

    fn is_excepted_domain(&self, domain: &str) -> bool {
        self.exceptions.iter().any(|exc| {
            if !exc.enabled {
                return false;
            }
            match &exc.exception_type {
                ExceptionType::Domain(d) => host_matches(d, domain),
                ExceptionType::Wildcard(pattern) => wildcard_matches(pattern, domain),
                ExceptionType::LocalFiles => false, // N/A for domain checks
            }
        })
    }
}

impl WebsiteRule {
    /// Check if this rule matches a given domain.
    pub fn matches_domain(&self, domain: &str) -> bool {
        match &self.match_type {
            WebsiteMatchType::Domain(d) => host_matches(d, domain),
            WebsiteMatchType::Wildcard(pattern) => wildcard_matches(pattern, domain),
            WebsiteMatchType::Keyword(kw) => domain.contains(&kw.to_lowercase()),
            // Domain-only check, so only the host part of the pattern applies.
            WebsiteMatchType::UrlPath(path) => host_matches(path, domain),
            WebsiteMatchType::EntireInternet => true,
        }
    }

    /// Check if this rule matches a full URL.
    pub fn matches_url(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();

        match &self.match_type {
            // Compare hosts rather than searching the raw URL: `contains` would
            // also fire on notyoutube.com and on the domain appearing in a query
            // string.
            WebsiteMatchType::Domain(d) => host_matches(d, &url_lower),
            // Against the host as well as the URL, so `*.youtube.com` catches a
            // page on it. The extension matches both the same way.
            WebsiteMatchType::Wildcard(pattern) => {
                glob_match::glob_match(&pattern.to_lowercase(), &url_lower)
                    || wildcard_matches(pattern, &canonical_host(&url_lower))
            }
            WebsiteMatchType::Keyword(kw) => url_lower.contains(&kw.to_lowercase()),
            WebsiteMatchType::UrlPath(path) => url_lower.contains(&path.to_lowercase()),
            WebsiteMatchType::EntireInternet => true,
        }
    }
}

impl AppRule {
    /// Check if this rule matches a running process.
    pub fn matches_process(
        &self,
        process_name: &str,
        exe_path: Option<&str>,
        window_title: Option<&str>,
    ) -> bool {
        let name_lower = process_name.to_lowercase();

        match &self.match_type {
            AppMatchType::ExecutableName(name) => name_lower == name.to_lowercase(),
            AppMatchType::ExecutablePath(path) => exe_path
                .map(|p| p.to_lowercase() == path.to_lowercase())
                .unwrap_or(false),
            AppMatchType::WindowTitle(title) => window_title
                .map(|t| t.to_lowercase().contains(&title.to_lowercase()))
                .unwrap_or(false),
            AppMatchType::BundleId(_) => false, // macOS only, handled separately
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_blocking() {
        let mut list = BlockList::new("Test");
        list.websites.push(WebsiteRule::domain("reddit.com"));
        list.websites.push(WebsiteRule::keyword("game"));

        assert!(list.should_block_domain("reddit.com"));
        assert!(list.should_block_domain("www.reddit.com"));
        assert!(list.should_block_domain("old.reddit.com"));
        assert!(!list.should_block_domain("redditor.com"));

        assert!(list.should_block_domain("steamgames.com"));
        assert!(list.should_block_domain("game.co"));
        assert!(!list.should_block_domain("example.com"));
    }

    #[test]
    fn test_exception_overrides_block() {
        let mut list = BlockList::new("Test");
        list.websites.push(WebsiteRule::entire_internet());
        list.exceptions
            .push(crate::types::ExceptionRule::domain("example.com"));

        assert!(list.should_block_domain("reddit.com"));
        assert!(!list.should_block_domain("example.com"));
        assert!(!list.should_block_domain("sub.example.com"));
    }

    #[test]
    fn test_wildcard_rule() {
        let rule = WebsiteRule::wildcard("*.social.*");
        assert!(rule.matches_domain("www.social.network"));
    }

    #[test]
    fn test_app_blocking() {
        let mut list = BlockList::new("Apps");
        list.applications.push(AppRule::executable("steam.exe"));
        list.applications.push(AppRule::window_title("YouTube"));

        assert!(list.should_block_app("steam.exe", None, None));
        assert!(!list.should_block_app("chrome.exe", None, None));
        assert!(list.should_block_app("chrome.exe", None, Some("YouTube - Google Chrome")));
    }

    #[test]
    fn test_disabled_list_blocks_nothing() {
        let mut list = BlockList::new("Disabled");
        list.enabled = false;
        list.websites.push(WebsiteRule::domain("reddit.com"));
        list.applications.push(AppRule::executable("steam.exe"));

        assert!(!list.should_block_domain("reddit.com"));
        assert!(!list.should_block_app("steam.exe", None, None));
    }

    /// Build a schedule with non-empty time slots that never match the
    /// current time, so `is_active_now()` is deterministically false.
    /// `start == end` makes `contains_time` return false on that day, and
    /// every weekday is covered so the result doesn't depend on when the
    /// test runs.
    fn never_active_schedule() -> crate::types::Schedule {
        use chrono::{NaiveTime, Weekday};
        let zero = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let days = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ];
        crate::types::Schedule {
            id: crate::types::new_id(),
            name: "NeverActive".into(),
            time_slots: days
                .iter()
                .map(|d| crate::types::TimeSlot::new(*d, zero, zero))
                .collect(),
            enabled: true,
        }
    }

    /// Regression: a scheduled list whose schedule is currently inactive
    /// must not block apps. Previously `should_block_app` only consulted
    /// `enabled` and ignored the schedule, so apps got force-closed even
    /// outside the configured time slots (issue #1).
    #[test]
    fn test_scheduled_inactive_does_not_block_app() {
        let mut list = BlockList::new("Scheduled");
        list.enabled = true;
        list.applications.push(AppRule::executable("discord.exe"));
        list.schedule = Some(never_active_schedule());

        assert!(!list.is_effectively_active());
        assert!(!list.should_block_app("discord.exe", None, None));
    }

    /// Regression: same fix applied to `should_block_domain` for symmetry.
    /// Domain blocking through the hosts file already used
    /// `is_effectively_active`, but the in-memory `should_block_domain`
    /// path (used by other callers) was inconsistent.
    #[test]
    fn test_scheduled_inactive_does_not_block_domain() {
        let mut list = BlockList::new("Scheduled");
        list.enabled = true;
        list.websites.push(WebsiteRule::domain("reddit.com"));
        list.schedule = Some(never_active_schedule());

        assert!(!list.should_block_domain("reddit.com"));
    }

    /// A schedule with no time slots is treated as "always active" while
    /// the list is enabled — make sure the fix didn't regress that case.
    #[test]
    fn test_schedule_no_slots_still_blocks() {
        use crate::types::Schedule;

        let mut list = BlockList::new("AlwaysOn");
        list.enabled = true;
        list.applications.push(AppRule::executable("discord.exe"));
        list.websites.push(WebsiteRule::domain("reddit.com"));
        list.schedule = Some(Schedule {
            id: crate::types::new_id(),
            name: "Test".into(),
            time_slots: vec![],
            enabled: true,
        });

        assert!(list.is_effectively_active());
        assert!(list.should_block_app("discord.exe", None, None));
        assert!(list.should_block_domain("reddit.com"));
    }
}
