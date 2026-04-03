use crate::types::{AppMatchType, AppRule, BlockList, ExceptionType, WebsiteMatchType, WebsiteRule};

impl BlockList {
    /// Check if a domain should be blocked by this block list.
    /// Returns `true` if any website rule matches AND no exception overrides it.
    pub fn should_block_domain(&self, domain: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let domain_lower = domain.to_lowercase();

        // Check exceptions first
        if self.is_excepted_domain(&domain_lower) {
            return false;
        }

        // Check each website rule
        self.websites.iter().any(|rule| {
            rule.enabled && rule.matches_domain(&domain_lower)
        })
    }

    /// Check if an application should be blocked.
    pub fn should_block_app(&self, process_name: &str, exe_path: Option<&str>, window_title: Option<&str>) -> bool {
        if !self.enabled {
            return false;
        }

        self.applications.iter().any(|rule| {
            rule.enabled && rule.matches_process(process_name, exe_path, window_title)
        })
    }

    fn is_excepted_domain(&self, domain: &str) -> bool {
        self.exceptions.iter().any(|exc| {
            if !exc.enabled {
                return false;
            }
            match &exc.exception_type {
                ExceptionType::Domain(d) => {
                    let d_lower = d.to_lowercase();
                    domain == d_lower || domain.ends_with(&format!(".{d_lower}"))
                }
                ExceptionType::Wildcard(pattern) => {
                    glob_match::glob_match(&pattern.to_lowercase(), domain)
                }
                ExceptionType::LocalFiles => false, // N/A for domain checks
            }
        })
    }
}

impl WebsiteRule {
    /// Check if this rule matches a given domain.
    pub fn matches_domain(&self, domain: &str) -> bool {
        match &self.match_type {
            WebsiteMatchType::Domain(d) => {
                let d_lower = d.to_lowercase();
                domain == d_lower || domain.ends_with(&format!(".{d_lower}"))
            }
            WebsiteMatchType::Wildcard(pattern) => {
                glob_match::glob_match(&pattern.to_lowercase(), domain)
            }
            WebsiteMatchType::Keyword(kw) => domain.contains(&kw.to_lowercase()),
            WebsiteMatchType::UrlPath(path) => {
                // For domain-only checks, match the domain portion
                let path_lower = path.to_lowercase();
                if let Some(slash_pos) = path_lower.find('/') {
                    let path_domain = &path_lower[..slash_pos];
                    domain == path_domain || domain.ends_with(&format!(".{path_domain}"))
                } else {
                    domain == path_lower || domain.ends_with(&format!(".{path_lower}"))
                }
            }
            WebsiteMatchType::EntireInternet => true,
        }
    }

    /// Check if this rule matches a full URL.
    pub fn matches_url(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();

        match &self.match_type {
            WebsiteMatchType::Domain(d) => {
                let d_lower = d.to_lowercase();
                url_lower.contains(&d_lower)
            }
            WebsiteMatchType::Wildcard(pattern) => {
                glob_match::glob_match(&pattern.to_lowercase(), &url_lower)
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
            AppMatchType::ExecutablePath(path) => {
                exe_path
                    .map(|p| p.to_lowercase() == path.to_lowercase())
                    .unwrap_or(false)
            }
            AppMatchType::WindowTitle(title) => {
                window_title
                    .map(|t| t.to_lowercase().contains(&title.to_lowercase()))
                    .unwrap_or(false)
            }
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
        list.exceptions.push(crate::types::ExceptionRule::domain("example.com"));

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

        assert!(!list.should_block_domain("reddit.com"));
    }
}
