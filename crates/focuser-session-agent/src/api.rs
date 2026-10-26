//! Unprivileged compatibility bridge. No database or application executor.
use focuser_common::extension::BrowserType;
use focuser_ipc::*;
use std::{
    collections::HashSet,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
static CLIENT: OnceLock<Client> = OnceLock::new();
static SNAPSHOT: Mutex<Option<(Instant, AgentSnapshot)>> = Mutex::new(None);
pub fn initialize(client: Client) {
    let _ = CLIENT.set(client);
}
pub fn snapshot() -> Option<AgentSnapshot> {
    SNAPSHOT
        .lock()
        .ok()?
        .as_ref()
        .filter(|(at, _)| at.elapsed() < Duration::from_secs(5))
        .map(|(_, s)| s.clone())
}
pub fn update(snapshot: AgentSnapshot) {
    if let Ok(mut cache) = SNAPSHOT.lock() {
        *cache = Some((Instant::now(), snapshot));
    }
}
pub fn get_connected_browsers(_: u64) -> HashSet<BrowserType> {
    snapshot()
        .map(|s| s.connected.into_iter().collect())
        .unwrap_or_default()
}
pub fn get_safely_connected_browsers(_: u64) -> HashSet<BrowserType> {
    snapshot()
        .map(|s| s.safely_connected.into_iter().collect())
        .unwrap_or_default()
}
pub fn get_recovery_ready_browsers() -> HashSet<BrowserType> {
    snapshot()
        .map(|s| s.recovery_ready.into_iter().collect())
        .unwrap_or_default()
}
pub fn forget_browser(browser: &BrowserType) {
    if let Some(client) = CLIENT.get() {
        let _ = client.request_blocking(RequestBody::Agent(AgentRequest::ForgetBrowser {
            browser: browser.clone(),
        }));
    }
    if let Ok(mut cache) = SNAPSHOT.lock()
        && let Some((_, s)) = cache.as_mut()
    {
        s.connected.retain(|b| b != browser);
        s.safely_connected.retain(|b| b != browser);
        s.recovery_ready.retain(|b| b != browser);
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedReason {
    NotInstalled,
    IncognitoNotAllowed,
}
pub struct BrowserPrompt {
    pub browser: String,
    pub reason: ClosedReason,
    pub closing_in_secs: Option<u64>,
}
pub fn prompt_about_browser(prompt: BrowserPrompt) {
    crate::notify(&format!(
        "{}: {:?}. Extension coverage required.{}",
        prompt.browser,
        prompt.reason,
        prompt
            .closing_in_secs
            .map(|n| format!(" Browser closes in {n} seconds."))
            .unwrap_or_default()
    ));
}

fn allowed_origin(origin: Option<&str>) -> bool {
    match origin {
        None => true, // Store extensions and background fetches can omit Origin.
        Some(
            "chrome-extension://jpnhbpbcmagoonmaleppldmcnaibkbmj"
            | "chrome-extension://kdkmjbjegdcjdlbciifigfkifookbppg",
        ) => true,
        Some(value) => value
            .strip_prefix("moz-extension://")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()),
    }
}
fn parse(stream: &mut TcpStream) -> Result<BrowserRequest, &'static str> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = Vec::new();
    let header_end;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("timeout")?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| "timeout")?;
        let mut byte = [0];
        if stream.read(&mut byte).map_err(|_| "timeout")? == 0 {
            return Err("malformed");
        }
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            header_end = bytes.len();
            break;
        }
        if bytes.len() > 16 * 1024 {
            return Err("message_too_large");
        }
    }
    let header = std::str::from_utf8(&bytes).map_err(|_| "malformed")?;
    let mut lines = header.split("\r\n");
    let mut request = lines.next().ok_or("malformed")?.split_whitespace();
    let method = request.next().ok_or("malformed")?.to_owned();
    let path = request.next().ok_or("malformed")?.to_owned();
    if !matches!(method.as_str(), "GET" | "POST" | "OPTIONS")
        || !path.starts_with("/api/")
        || path.len() > 4096
    {
        return Err("unsupported");
    }
    if request.next() != Some("HTTP/1.1") || request.next().is_some() {
        return Err("malformed");
    }
    let mut length = None;
    let mut origin = None;
    let mut browser = None;
    let mut host = None;
    for line in lines.filter(|s| !s.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("malformed")?;
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "content-length" => {
                if length.is_some() {
                    return Err("malformed");
                }
                length = Some(value.parse::<usize>().map_err(|_| "malformed")?);
            }
            "transfer-encoding" => return Err("malformed"),
            "origin" => {
                if origin.replace(value).is_some() {
                    return Err("malformed");
                }
            }
            "host" => {
                if host.replace(value).is_some() {
                    return Err("malformed");
                }
            }
            "x-focuser-browser" => browser = Some(value.to_owned()),
            "sec-fetch-dest" if value != "empty" => return Err("unauthorized"),
            "sec-fetch-mode" if !matches!(value, "cors" | "same-origin") => {
                return Err("unauthorized");
            }
            _ => {}
        }
    }
    if !allowed_origin(origin) {
        return Err("unauthorized");
    }
    let port = stream.local_addr().map_err(|_| "malformed")?.port();
    if host != Some(format!("127.0.0.1:{port}").as_str())
        && host != Some(format!("localhost:{port}").as_str())
    {
        return Err("unauthorized");
    }
    let length = length.unwrap_or(0);
    if length > 64 * 1024 {
        return Err("message_too_large");
    }
    stream
        .set_read_timeout(Some(
            deadline
                .checked_duration_since(Instant::now())
                .ok_or("timeout")?,
        ))
        .map_err(|_| "timeout")?;
    bytes.resize(header_end + length, 0);
    let mut read = header_end;
    while read < bytes.len() {
        stream
            .set_read_timeout(Some(
                deadline
                    .checked_duration_since(Instant::now())
                    .ok_or("timeout")?,
            ))
            .map_err(|_| "timeout")?;
        let count = stream.read(&mut bytes[read..]).map_err(|_| "timeout")?;
        if count == 0 {
            return Err("malformed");
        }
        read += count;
    }
    let body = String::from_utf8(bytes[header_end..].to_vec()).map_err(|_| "malformed")?;
    Ok(BrowserRequest {
        method,
        path,
        body,
        browser,
    })
}
fn reply(stream: &mut TcpStream, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {status} Focuser\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type, X-Focuser-Browser\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = response.as_bytes();
    while !bytes.is_empty() {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        if stream.set_write_timeout(Some(remaining)).is_err() {
            break;
        }
        match stream.write(bytes) {
            Ok(0) | Err(_) => break,
            Ok(count) => bytes = &bytes[count..],
        }
    }
}
pub fn serve(listener: TcpListener, client: Client) -> std::io::Result<()> {
    let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for stream in listener.incoming() {
        let mut stream = stream?;
        if active.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 16 {
            active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            continue;
        }
        let active = active.clone();
        let client = client.clone();
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
            let result = (|| {
                let peer = stream
                    .peer_addr()
                    .map_err(|_| IpcError::new("unauthorized", "No peer"))?;
                if !crate::observation::http_peer(
                    peer,
                    stream
                        .local_addr()
                        .map_err(|_| IpcError::new("transport", "No local address"))?
                        .port(),
                ) {
                    return Err(IpcError::new(
                        "unauthorized",
                        "TCP owner is outside this interactive session",
                    ));
                }
                let request =
                    parse(&mut stream).map_err(|e| IpcError::new(e, "Invalid browser request"))?;
                if request.path == "/api/show" {
                    crate::show_ui();
                }
                match client.request_blocking(RequestBody::Browser(request))? {
                    ReplyBody::Browser(reply) => Ok(reply),
                    _ => Err(IpcError::new("protocol", "Unexpected reply")),
                }
            })();
            match result {
                Ok(result) => reply(&mut stream, result.status, &result.body),
                Err(error) => reply(
                    &mut stream,
                    if error.code == "unauthorized" {
                        403
                    } else {
                        503
                    },
                    &serde_json::json!({"error":error.code,"message":error.message}).to_string(),
                ),
            }
            active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_web_and_null_origins() {
        for origin in [
            "https://example.com",
            "null",
            "http://localhost:3000",
            "moz-extension://fake",
        ] {
            assert!(!allowed_origin(Some(origin)));
        }
        assert!(allowed_origin(None));
        assert!(allowed_origin(Some(
            "chrome-extension://jpnhbpbcmagoonmaleppldmcnaibkbmj"
        )));
    }

    fn parse_request(headers: &str) -> Result<BrowserRequest, &'static str> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let text = headers.replace("$HOST", &address.to_string());
        let writer = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            let _ = stream.write_all(text.as_bytes());
        });
        let (mut stream, _) = listener.accept().unwrap();
        let result = parse(&mut stream);
        writer.join().unwrap();
        result
    }
    #[test]
    fn rejects_raw_commands_oversized_bodies_and_ambiguous_framing() {
        assert_eq!(
            parse_request("POST /command HTTP/1.1\r\nHost: $HOST\r\n\r\n").unwrap_err(),
            "unsupported"
        );
        assert_eq!(
            parse_request(
                "POST /api/add-site HTTP/1.1\r\nHost: $HOST\r\nContent-Length: 65537\r\n\r\n"
            )
            .unwrap_err(),
            "message_too_large"
        );
        assert_eq!(parse_request("POST /api/add-site HTTP/1.1\r\nHost: $HOST\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n").unwrap_err(),"malformed");
        assert_eq!(
            parse_request("GET /api/rules HTTP/1.1\r\nHost: attacker.example\r\n\r\n").unwrap_err(),
            "unauthorized"
        );
        assert_eq!(
            parse_request(
                "GET /api/rules HTTP/1.1\r\nHost: $HOST\r\nOrigin: https://example.com\r\n\r\n"
            )
            .unwrap_err(),
            "unauthorized"
        );
        assert!(parse_request("GET /api/rules HTTP/1.1\r\nHost: $HOST\r\n\r\n").is_ok());
    }
}
