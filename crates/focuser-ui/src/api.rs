//! Local HTTP API server for browser extension communication.
//!
//! Listens on 127.0.0.1:17549. The browser extension calls these endpoints
//! to get blocking rules, add/remove sites, and check domain status.
//!
//! Also tracks which browsers have a connected extension via the
//! `X-Focuser-Browser` header sent by the extension on each request.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use focuser_common::extension::BrowserType;
use tracing::{debug, error, info};

use crate::AppState;
use focuser_common::host::{any_host_matches, canonical_host};

use crate::blocker;

/// Flag to request the main window to show itself.
pub static SHOW_WINDOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Flag + browser name for showing "install extension" prompt.
pub static EXTENSION_PROMPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Stores the name of the browser that was killed (for the prompt message).
static KILLED_BROWSER_NAME: Mutex<Option<String>> = Mutex::new(None);

/// Tracks last-seen time for each browser's extension.
/// Updated when the extension sends an API request with the `X-Focuser-Browser` header.
static CONNECTED_BROWSERS: Mutex<Option<HashMap<BrowserType, Instant>>> = Mutex::new(None);

/// Server-side dedup window for blocked reports.
/// Prevents double-counting from:
/// - Multi-tab: multiple tabs of the same site reporting nearly simultaneously
/// - Browser navigation events: onCommitted + onCompleted for the same URL
/// - Service worker restarts: extension's local dedup state lost
/// - Chrome retrying failed loads
static RECENT_REPORTS: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
const REPORT_DEDUP_WINDOW_SECS: u64 = 3;

/// Returns true if this domain should be recorded (not a duplicate of a recent report).
fn should_record_blocked(tracking_key: &str) -> bool {
    let now = Instant::now();
    let mut guard = RECENT_REPORTS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);

    // Cleanup old entries (keep only recent ones)
    let window = std::time::Duration::from_secs(REPORT_DEDUP_WINDOW_SECS * 4);
    map.retain(|_, ts| now.duration_since(*ts) < window);

    if let Some(last) = map.get(tracking_key)
        && now.duration_since(*last) < std::time::Duration::from_secs(REPORT_DEDUP_WINDOW_SECS)
    {
        return false;
    }
    map.insert(tracking_key.to_string(), now);
    true
}

/// Record that a browser's extension was seen (called from API request handler).
fn record_extension_heartbeat(browser_name: &str) {
    let name_lower = browser_name.to_lowercase();
    let browser_type = match name_lower.as_str() {
        "chrome" => BrowserType::Chrome,
        "firefox" => BrowserType::Firefox,
        "edge" => BrowserType::Edge,
        "brave" => BrowserType::Brave,
        "opera" => BrowserType::Opera,
        _ => return,
    };

    let mut guard = CONNECTED_BROWSERS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(browser_type, Instant::now());
    debug!(browser = browser_name, "Extension heartbeat recorded");
}

/// Get the set of browsers whose extension was seen within the timeout.
pub fn get_connected_browsers(timeout_secs: u64) -> std::collections::HashSet<BrowserType> {
    let cutoff = std::time::Duration::from_secs(timeout_secs);
    let now = Instant::now();
    let guard = CONNECTED_BROWSERS.lock().unwrap();

    match guard.as_ref() {
        Some(map) => map
            .iter()
            .filter(|(_, last_seen)| now.duration_since(**last_seen) < cutoff)
            .map(|(bt, _)| bt.clone())
            .collect(),
        None => std::collections::HashSet::new(),
    }
}

/// Set the killed browser name for the prompt.
pub fn set_killed_browser(name: &str) {
    if let Ok(mut guard) = KILLED_BROWSER_NAME.lock() {
        *guard = Some(name.to_string());
    }
    EXTENSION_PROMPT_REQUESTED.store(true, Ordering::Relaxed);
}

/// Take the killed browser name (resets it).
pub fn take_killed_browser() -> Option<String> {
    if let Ok(mut guard) = KILLED_BROWSER_NAME.lock() {
        guard.take()
    } else {
        None
    }
}

const API_PORT: u16 = 17549;

/// Start the API server. Runs forever — call from a spawned thread.
pub fn run_api_server(state: Arc<AppState>) {
    let listener = match TcpListener::bind(format!("127.0.0.1:{API_PORT}")) {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, port = API_PORT, "Cannot start extension API server");
            return;
        }
    };
    info!(port = API_PORT, "Extension API server listening");
    serve(listener, state);
}

/// Serve on an already-bound listener.
///
/// Split out so tests can bind an ephemeral port and drive the real endpoints
/// over real HTTP — the extension's half of the contract is otherwise only
/// exercised by installing the extension and watching.
pub fn serve(listener: TcpListener, state: Arc<AppState>) {
    for stream in listener.incoming().flatten() {
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            handle_request(stream, &state);
        });
    }
}

fn handle_request(mut stream: std::net::TcpStream, state: &AppState) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));

    let mut reader = BufReader::new(&stream);

    // Read request line
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }

    // Read headers, extract Content-Length and X-Focuser-Browser
    let mut content_length: usize = 0;
    let mut focuser_browser: Option<String> = None;
    let mut header = String::new();
    loop {
        header.clear();
        if reader.read_line(&mut header).unwrap_or(0) == 0 {
            break;
        }
        if header.trim().is_empty() {
            break;
        }
        if let Some(val) = header.strip_prefix("Content-Length:") {
            content_length = val.trim().parse().unwrap_or(0);
        }
        if let Some(val) = header.strip_prefix("content-length:") {
            content_length = val.trim().parse().unwrap_or(0);
        }
        // Case-insensitive header name match for X-Focuser-Browser
        if let Some(pos) = header.to_lowercase().find("x-focuser-browser:") {
            let val_start = pos + "x-focuser-browser:".len();
            if val_start < header.len() {
                focuser_browser = Some(header[val_start..].trim().to_string());
            }
        }
    }

    // Track connected browser extension
    if let Some(ref browser_name) = focuser_browser {
        record_extension_heartbeat(browser_name);
    }

    // Read body if present
    let mut body = String::new();
    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        use std::io::Read;
        let _ = reader.read_exact(&mut buf);
        body = String::from_utf8_lossy(&buf).to_string();
    }

    // Parse method and path
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("");
    let path = parts.get(1).copied().unwrap_or("");

    // Route
    let (status, response_body) = route(method, path, &body, state);

    let response = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        response_body.len(),
        response_body,
    );

    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn route(method: &str, path: &str, body: &str, state: &AppState) -> (&'static str, String) {
    // Handle CORS preflight
    if method == "OPTIONS" {
        return ("204 No Content", String::new());
    }

    match (method, path) {
        ("GET", "/api/status") => api_status(state),
        ("GET", "/api/lists") => api_lists(state),
        ("GET", "/api/rules") => api_rules(state),
        ("POST", "/api/add-site") => api_add_site(body, state),
        ("POST", "/api/remove-site") => api_remove_site(body, state),
        ("POST", "/api/toggle-list") => api_toggle_list(body, state),
        ("POST", "/api/blocked") => api_report_blocked(body, state),
        ("POST", "/api/allowance-tick") => api_allowance_tick(body, state),
        ("GET", "/api/allowance-blocked") => api_allowance_blocked(state),
        ("GET", "/api/pomodoro") => api_pomodoro_status(state),
        ("GET", "/api/allowances") => api_allowances_list(state),
        ("POST", "/api/show") | ("GET", "/api/show") => {
            SHOW_WINDOW_REQUESTED.store(true, Ordering::Relaxed);
            ("200 OK", r#"{"ok":true}"#.into())
        }
        _ if path.starts_with("/api/check?") => api_check_domain(path, state),
        _ if path.starts_with("/api/blocked-count?") => api_blocked_count(path, state),
        _ if path.starts_with("/api/heartbeat?") => {
            // Dedicated heartbeat endpoint — browser identified via URL, not headers
            let browser = path
                .split("browser=")
                .nth(1)
                .unwrap_or("")
                .split('&')
                .next()
                .unwrap_or("");
            if !browser.is_empty() {
                record_extension_heartbeat(browser);
            }
            ("200 OK", r#"{"ok":true}"#.into())
        }
        _ => ("404 Not Found", r#"{"error":"not found"}"#.into()),
    }
}

fn api_status(state: &AppState) -> (&'static str, String) {
    let eng = state.engine.lock().unwrap();
    let lists = eng.block_lists();
    let active_count = lists.iter().filter(|l| l.enabled).count();
    let total_sites: usize = lists
        .iter()
        .filter(|l| l.enabled)
        .map(|l| l.websites.len())
        .sum();
    let total_apps: usize = lists
        .iter()
        .filter(|l| l.enabled)
        .map(|l| l.applications.len())
        .sum();
    let blocked_today = eng.db().get_total_blocked_today().unwrap_or(0);

    let json = serde_json::json!({
        "running": true,
        "active_lists": active_count,
        "blocked_sites": total_sites,
        "blocked_apps": total_apps,
        "blocked_today": blocked_today,
    });
    ("200 OK", json.to_string())
}

fn api_lists(state: &AppState) -> (&'static str, String) {
    let eng = state.engine.lock().unwrap();
    let lists: Vec<serde_json::Value> = eng
        .block_lists()
        .iter()
        .map(|l| {
            serde_json::json!({
                "id": l.id.to_string(),
                "name": l.name,
                "enabled": l.enabled,
                "website_count": l.websites.len(),
                "app_count": l.applications.len(),
            })
        })
        .collect();
    ("200 OK", serde_json::to_string(&lists).unwrap_or_default())
}

fn api_rules(state: &AppState) -> (&'static str, String) {
    let eng = state.engine.lock().unwrap();
    let allowance_exceptions = state.allowance_exempt_domains(&eng);
    let rules = eng.compile_extension_rules_with_exceptions(&allowance_exceptions);

    let mut domain_categories: HashMap<String, String> = HashMap::new();
    for list in eng.block_lists().iter().filter(|l| l.enabled) {
        let category = normalize_category(&list.name);
        for rule in &list.websites {
            if let focuser_common::types::WebsiteMatchType::Domain(ref d) = rule.match_type {
                domain_categories.insert(canonical_host(d), category.clone());
            }
        }
    }

    let mut response = serde_json::to_value(&rules).unwrap_or_default();
    if let Some(obj) = response.as_object_mut() {
        obj.insert(
            "domain_categories".to_string(),
            serde_json::to_value(&domain_categories).unwrap_or_default(),
        );
    }

    ("200 OK", response.to_string())
}

fn normalize_category(list_name: &str) -> String {
    let name = list_name.to_lowercase();
    for (keyword, category) in [
        ("porn", "adult"),
        ("adult", "adult"),
        ("nsfw", "adult"),
        ("xxx", "adult"),
        ("social", "social_media"),
        ("instagram", "social_media"),
        ("facebook", "social_media"),
        ("twitter", "social_media"),
        ("tiktok", "social_media"),
        ("video", "video"),
        ("stream", "video"),
        ("youtube", "video"),
        ("netflix", "video"),
        ("entertain", "video"),
        ("dating", "dating"),
        ("tinder", "dating"),
        ("bumble", "dating"),
        ("gaming", "gaming"),
        ("game", "gaming"),
        ("steam", "gaming"),
        ("news", "news"),
        ("reddit", "news"),
        ("forum", "news"),
        ("shop", "shopping"),
        ("amazon", "shopping"),
        ("ebay", "shopping"),
        ("gambling", "gambling"),
        ("betting", "gambling"),
        ("casino", "gambling"),
    ] {
        if name.contains(keyword) {
            return category.to_string();
        }
    }
    "default".to_string()
}

fn api_check_domain(path: &str, state: &AppState) -> (&'static str, String) {
    let domain = path
        .split("domain=")
        .nth(1)
        .unwrap_or("")
        .split('&')
        .next()
        .unwrap_or("");

    let eng = state.engine.lock().unwrap();
    let exemptions = state.allowance_exempt_domains(&eng);
    let blocked = if any_host_matches(&exemptions, domain) {
        false
    } else {
        eng.check_domain(domain).is_some()
    };
    let json = serde_json::json!({ "domain": domain, "blocked": blocked });
    ("200 OK", json.to_string())
}

fn api_add_site(body: &str, state: &AppState) -> (&'static str, String) {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let list_id = parsed["list_id"].as_str().unwrap_or("");
    let domain = parsed["domain"].as_str().unwrap_or("");
    let rule_type = parsed["rule_type"].as_str().unwrap_or("domain");

    if list_id.is_empty() || domain.is_empty() {
        return (
            "400 Bad Request",
            r#"{"error":"list_id and domain required"}"#.into(),
        );
    }

    let uuid = match uuid::Uuid::parse_str(list_id) {
        Ok(u) => u,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let mut eng = state.engine.lock().unwrap();
    let mut list = match eng.db().get_block_list(uuid) {
        Ok(l) => l,
        Err(e) => return ("404 Not Found", format!(r#"{{"error":"{}"}}"#, e)),
    };

    use focuser_common::types::WebsiteRule;
    let rule = match rule_type {
        "keyword" => WebsiteRule::keyword(domain),
        "wildcard" => WebsiteRule::wildcard(domain),
        "url_path" => WebsiteRule::url_path(domain),
        _ => WebsiteRule::domain(domain),
    };

    list.websites.push(rule);
    list.updated_at = chrono::Utc::now();

    if let Err(e) = eng.db().update_block_list(&list) {
        return (
            "500 Internal Server Error",
            format!(r#"{{"error":"{}"}}"#, e),
        );
    }
    let _ = eng.refresh();
    let _ = blocker::apply_hosts_blocks(&eng.collect_blocked_domains());

    ("200 OK", r#"{"ok":true}"#.into())
}

fn api_remove_site(body: &str, state: &AppState) -> (&'static str, String) {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let list_id = parsed["list_id"].as_str().unwrap_or("");
    let rule_id = parsed["rule_id"].as_str().unwrap_or("");
    let domain = parsed["domain"].as_str().unwrap_or("");

    // Support removal by rule_id OR by domain name
    if list_id.is_empty() || (rule_id.is_empty() && domain.is_empty()) {
        return (
            "400 Bad Request",
            r#"{"error":"list_id and (rule_id or domain) required"}"#.into(),
        );
    }

    let uuid = match uuid::Uuid::parse_str(list_id) {
        Ok(u) => u,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let mut eng = state.engine.lock().unwrap();
    let mut list = match eng.db().get_block_list(uuid) {
        Ok(l) => l,
        Err(e) => return ("404 Not Found", format!(r#"{{"error":"{}"}}"#, e)),
    };

    if !rule_id.is_empty() {
        list.websites.retain(|r| r.id.to_string() != rule_id);
    } else if !domain.is_empty() {
        let domain_lower = domain.to_lowercase();
        list.websites.retain(|r| match &r.match_type {
            focuser_common::types::WebsiteMatchType::Domain(d) => d.to_lowercase() != domain_lower,
            _ => true,
        });
    }
    list.updated_at = chrono::Utc::now();

    if let Err(e) = eng.db().update_block_list(&list) {
        return (
            "500 Internal Server Error",
            format!(r#"{{"error":"{}"}}"#, e),
        );
    }
    let _ = eng.refresh();
    let _ = blocker::apply_hosts_blocks(&eng.collect_blocked_domains());

    ("200 OK", r#"{"ok":true}"#.into())
}

fn api_report_blocked(body: &str, state: &AppState) -> (&'static str, String) {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let domain = parsed["domain"].as_str().unwrap_or("");
    if domain.is_empty() {
        return ("400 Bad Request", r#"{"error":"domain required"}"#.into());
    }

    let tracking_key = parsed["tracking_key"].as_str().unwrap_or(domain);

    // Server-side dedup: only record if this exact tracking_key wasn't recorded
    // within the last REPORT_DEDUP_WINDOW_SECS seconds. This handles multi-tab,
    // navigation redirects, service worker restarts, and duplicate event sources.
    let recorded = should_record_blocked(tracking_key);

    let eng = state.engine.lock().unwrap();

    if recorded {
        let _ = eng.record_blocked(domain);
        if tracking_key != domain {
            let _ = eng.db().record_blocked_attempt(tracking_key);
        }
    }

    // Always return the current count (whether or not we just incremented).
    let count = if tracking_key != domain {
        eng.db().get_blocked_count_today(tracking_key).unwrap_or(0)
    } else {
        eng.db().get_blocked_count_today(domain).unwrap_or(0)
    };

    let json = serde_json::json!({ "ok": true, "count": count, "recorded": recorded });
    ("200 OK", json.to_string())
}

fn api_blocked_count(path: &str, state: &AppState) -> (&'static str, String) {
    let key = path
        .split("key=")
        .nth(1)
        .unwrap_or("")
        .split('&')
        .next()
        .unwrap_or("");
    let decoded = percent_decode(key);
    let eng = state.engine.lock().unwrap();
    let count = eng.db().get_blocked_count_today(&decoded).unwrap_or(0);
    let json = serde_json::json!({ "key": decoded, "count": count });
    ("200 OK", json.to_string())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(h) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(h, 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn api_toggle_list(body: &str, state: &AppState) -> (&'static str, String) {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let list_id = parsed["list_id"].as_str().unwrap_or("");
    let enabled = parsed["enabled"].as_bool().unwrap_or(true);

    let uuid = match uuid::Uuid::parse_str(list_id) {
        Ok(u) => u,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let mut eng = state.engine.lock().unwrap();
    let mut list = match eng.db().get_block_list(uuid) {
        Ok(l) => l,
        Err(e) => return ("404 Not Found", format!(r#"{{"error":"{}"}}"#, e)),
    };

    list.enabled = enabled;
    list.updated_at = chrono::Utc::now();

    if let Err(e) = eng.db().update_block_list(&list) {
        return (
            "500 Internal Server Error",
            format!(r#"{{"error":"{}"}}"#, e),
        );
    }
    let _ = eng.refresh();
    let _ = blocker::apply_hosts_blocks(&eng.collect_blocked_domains());

    ("200 OK", r#"{"ok":true}"#.into())
}

fn api_allowance_tick(body: &str, state: &AppState) -> (&'static str, String) {
    let tick: focuser_common::allowance::AllowanceTick = match serde_json::from_str(body) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!(error = %e, body = %body, "bad allowance-tick payload");
            return ("400 Bad Request", format!(r#"{{"error":"{e}"}}"#));
        }
    };
    let eng = match state.engine.lock() {
        Ok(e) => e,
        Err(_) => return ("500 Internal Server Error", r#"{"error":"lock"}"#.into()),
    };
    if let Err(e) = state.allowance_tracker.ingest_tick(eng.db(), &tick) {
        tracing::warn!(error = %e, "allowance-tick ingest failed");
        return ("500 Internal Server Error", format!(r#"{{"error":"{e}"}}"#));
    }
    tracing::debug!(
        host = ?tick.hostname,
        app = ?tick.app_exe,
        active = tick.active,
        inc = ?tick.increment_secs,
        "allowance tick ingested"
    );
    ("200 OK", r#"{"ok":true}"#.into())
}

fn api_allowance_blocked(state: &AppState) -> (&'static str, String) {
    let domains = state.allowance_tracker.blocked_domains();
    let apps = state.allowance_tracker.blocked_apps();
    let json = serde_json::json!({ "domains": domains, "apps": apps });
    ("200 OK", json.to_string())
}

fn api_pomodoro_status(state: &AppState) -> (&'static str, String) {
    let eng = match state.engine.lock() {
        Ok(e) => e,
        Err(_) => return ("500 Internal Server Error", r#"{"error":"lock"}"#.into()),
    };
    match focuser_core::pomodoro::build_status(eng.db()) {
        Ok(Some(s)) => (
            "200 OK",
            serde_json::to_string(&s).unwrap_or_else(|_| "null".into()),
        ),
        Ok(None) => ("200 OK", "null".into()),
        Err(e) => ("500 Internal Server Error", format!(r#"{{"error":"{e}"}}"#)),
    }
}

fn api_allowances_list(state: &AppState) -> (&'static str, String) {
    let eng = match state.engine.lock() {
        Ok(e) => e,
        Err(_) => return ("500 Internal Server Error", r#"{"error":"lock"}"#.into()),
    };
    match eng.db().list_allowance_statuses() {
        Ok(list) => (
            "200 OK",
            serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()),
        ),
        Err(e) => ("500 Internal Server Error", format!(r#"{{"error":"{e}"}}"#)),
    }
}

#[cfg(test)]
mod tests {
    //! Drives the real extension endpoints over real HTTP.
    //!
    //! This is the half of the allowance feature that used to be verifiable
    //! only by installing the extension and watching a number fail to move.
    //! The requests below are byte-for-byte what `background.js` sends.

    use std::io::{Read, Write};
    use std::sync::Arc;

    use focuser_app::AppContext;
    use focuser_common::allowance::AllowanceMatch;
    use focuser_common::types::{BlockList, WebsiteRule};
    use focuser_core::{BlockEngine, Database};

    /// A context that reports a connected extension, since that is the state
    /// any request to these endpoints implies.
    fn ctx_with_extension(setup: impl FnOnce(&Database)) -> Arc<AppContext> {
        struct Connected;
        impl focuser_app::SystemSync for Connected {
            fn sync_hosts(&self, _domains: &[String]) {}
            fn connected_browsers(&self) -> Vec<String> {
                vec!["Chrome".to_string()]
            }
        }

        let db = Database::open_in_memory().unwrap();
        setup(&db);
        let engine = BlockEngine::new(db).unwrap();
        Arc::new(AppContext::new(engine, Arc::new(Connected)))
    }

    /// Start the server on an ephemeral port and return its address.
    fn start(state: Arc<AppContext>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || super::serve(listener, state));
        addr
    }

    fn request(addr: &str, method: &str, path: &str, body: Option<&str>) -> String {
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let body = body.unwrap_or("");
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or(response)
    }

    fn tick(addr: &str, hostname: &str, seconds: u32) -> String {
        // Exactly the payload background.js posts.
        let body = format!(
            r#"{{"hostname":"{hostname}","app_exe":null,"active":true,\
               "source":"extension-tick","increment_secs":{seconds}}}"#
        )
        .replace("\\\n               ", "");
        request(addr, "POST", "/api/allowance-tick", Some(&body))
    }

    fn used_secs(state: &AppContext) -> u32 {
        let eng = state.engine.lock().unwrap();
        let allowances = eng.db().list_allowances().unwrap();
        eng.db().get_allowance_used_today(allowances[0].id).unwrap()
    }

    #[test]
    fn a_tick_increments_the_matching_allowance_whichever_www_form_it_reports() {
        for stored in ["youtube.com", "www.youtube.com"] {
            let state = ctx_with_extension(|db| {
                db.create_allowance(&focuser_common::allowance::Allowance::new(
                    AllowanceMatch::Domain(stored.into()),
                    1800,
                    true,
                ))
                .unwrap();
            });
            let addr = start(Arc::clone(&state));

            assert_eq!(used_secs(&state), 0, "starts at zero");

            for reported in ["youtube.com", "www.youtube.com", "music.youtube.com"] {
                tick(&addr, reported, 5);
            }

            assert_eq!(
                used_secs(&state),
                15,
                "allowance stored as {stored:?} should count every form"
            );
        }
    }

    #[test]
    fn a_tick_for_an_unrelated_site_counts_nothing() {
        let state = ctx_with_extension(|db| {
            db.create_allowance(&focuser_common::allowance::Allowance::new(
                AllowanceMatch::Domain("youtube.com".into()),
                1800,
                true,
            ))
            .unwrap();
        });
        let addr = start(Arc::clone(&state));

        tick(&addr, "example.com", 30);
        tick(&addr, "notyoutube.com", 30);

        assert_eq!(used_secs(&state), 0);
    }

    #[test]
    fn usage_accumulates_until_the_limit_then_the_site_is_blocked() {
        let state = ctx_with_extension(|db| {
            let mut list = BlockList::new("Videos");
            list.websites.push(WebsiteRule::domain("youtube.com"));
            db.create_block_list(&list).unwrap();
            db.create_allowance(&focuser_common::allowance::Allowance::new(
                AllowanceMatch::Domain("www.youtube.com".into()),
                60,
                true,
            ))
            .unwrap();
        });
        let addr = start(Arc::clone(&state));

        // Under the limit: reachable, because the allowance exempts it.
        let body = request(&addr, "GET", "/api/check?domain=www.youtube.com", None);
        assert!(
            body.contains(r#""blocked":false"#),
            "should be reachable while the budget lasts, got {body}"
        );

        // Burn the whole minute.
        for _ in 0..2 {
            tick(&addr, "www.youtube.com", 30);
        }
        assert_eq!(used_secs(&state), 60);

        // Exhausted: blocked again, in either form.
        for host in ["youtube.com", "www.youtube.com"] {
            let body = request(&addr, "GET", &format!("/api/check?domain={host}"), None);
            assert!(
                body.contains(r#""blocked":true"#),
                "{host} should be blocked once the budget is spent, got {body}"
            );
        }
    }

    #[test]
    fn strict_mode_ignores_a_tick_from_an_unfocused_tab() {
        let state = ctx_with_extension(|db| {
            db.create_allowance(&focuser_common::allowance::Allowance::new(
                AllowanceMatch::Domain("youtube.com".into()),
                1800,
                true,
            ))
            .unwrap();
        });
        let addr = start(Arc::clone(&state));

        request(
            &addr,
            "POST",
            "/api/allowance-tick",
            Some(r#"{"hostname":"youtube.com","app_exe":null,"active":false,"increment_secs":30}"#),
        );

        assert_eq!(used_secs(&state), 0, "strict mode only counts focused time");
    }

    #[test]
    fn compiled_rules_carry_both_www_forms_for_blocks_and_exemptions() {
        let state = ctx_with_extension(|db| {
            let mut list = BlockList::new("Videos");
            list.websites.push(WebsiteRule::domain("youtube.com"));
            db.create_block_list(&list).unwrap();
            db.create_allowance(&focuser_common::allowance::Allowance::new(
                AllowanceMatch::Domain("www.youtube.com".into()),
                1800,
                true,
            ))
            .unwrap();
        });
        let addr = start(Arc::clone(&state));

        let body = request(&addr, "GET", "/api/rules", None);
        let rules: serde_json::Value = serde_json::from_str(&body).unwrap();

        let blocked = rules["blocked_domains"].as_array().unwrap();
        let allowed = rules["allowed_domains"].as_array().unwrap();

        for form in ["youtube.com", "www.youtube.com"] {
            assert!(
                blocked.iter().any(|d| d == form),
                "blocked_domains should list {form}: {blocked:?}"
            );
            assert!(
                allowed.iter().any(|d| d == form),
                "allowed_domains should list {form}: {allowed:?}"
            );
        }
    }
}
