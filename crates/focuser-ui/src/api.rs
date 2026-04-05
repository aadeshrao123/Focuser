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

/// Flag to request the main window to show itself.
pub static SHOW_WINDOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Flag + browser name for showing "install extension" prompt.
pub static EXTENSION_PROMPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Stores the name of the browser that was killed (for the prompt message).
static KILLED_BROWSER_NAME: Mutex<Option<String>> = Mutex::new(None);

/// Tracks last-seen time for each browser's extension.
/// Updated when the extension sends an API request with the `X-Focuser-Browser` header.
static CONNECTED_BROWSERS: Mutex<Option<HashMap<BrowserType, Instant>>> = Mutex::new(None);

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
        ("POST", "/api/show") | ("GET", "/api/show") => {
            SHOW_WINDOW_REQUESTED.store(true, Ordering::Relaxed);
            ("200 OK", r#"{"ok":true}"#.into())
        }
        _ if path.starts_with("/api/check?") => api_check_domain(path, state),
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
    let rules = eng.compile_extension_rules();
    ("200 OK", serde_json::to_string(&rules).unwrap_or_default())
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
    let blocked = eng.check_domain(domain).is_some();
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
    crate::commands::sync_hosts_now_static(&eng);

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
    crate::commands::sync_hosts_now_static(&eng);

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

    let eng = state.engine.lock().unwrap();
    let _ = eng.record_blocked(domain);
    ("200 OK", r#"{"ok":true}"#.into())
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
    crate::commands::sync_hosts_now_static(&eng);

    ("200 OK", r#"{"ok":true}"#.into())
}
