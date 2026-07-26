//! Development-only HTTP bridge to the command core.
//!
//! Lets the React frontend run in an ordinary browser at `localhost:1420` and
//! drive **real** Rust command logic, rather than mocks that quietly drift from
//! the backend. Same `execute()` the Tauri app and CLI call — this is just a
//! fourth thin shim over it.
//!
//! Why this matters: without it, exercising the UI means launching the desktop
//! app and clicking by hand. With it, the UI is reachable by browser automation,
//! so nearly all behaviour can be tested without a human.
//!
//! ```text
//!   browser (localhost:1420)  ──HTTP──>  devserver (17550)  ──>  execute()
//!   Tauri webview             ──IPC───>  #[tauri::command]   ──>  execute()
//!   terminal                  ──────────>  clap subcommand    ──>  execute()
//! ```
//!
//! Not shipped: `publish = false`, and nothing in the release bundle depends on
//! it. It binds loopback only.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use focuser_app::{AppContext, Command, execute};
use focuser_core::{BlockEngine, Database};
use tracing::{Level, error, info, warn};

/// Deliberately not 17549 — that port is contested by the service's IPC
/// listener and the Tauri app's extension API, and inheriting that collision
/// in a dev tool would only confuse debugging.
const DEFAULT_PORT: u16 = 17550;

/// The Vite dev server. Only this origin is allowed through CORS.
const DEV_ORIGIN: &str = "http://localhost:1420";

#[derive(Parser)]
#[command(
    name = "focuser-devserver",
    about = "Dev-only HTTP bridge to the Focuser command core"
)]
struct Cli {
    /// Port to listen on (loopback only).
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// SQLite file to use. Defaults to a throwaway dev database so testing
    /// never touches real block lists.
    #[arg(long)]
    db: Option<PathBuf>,

    /// Use an in-memory database — state resets each run, for repeatable tests.
    #[arg(long)]
    memory: bool,
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    let cli = Cli::parse();

    let db = if cli.memory {
        info!("using in-memory database (state resets on exit)");
        Database::open_in_memory().expect("failed to open in-memory database")
    } else {
        let path = cli.db.unwrap_or_else(|| PathBuf::from("focuser-dev.db"));
        info!(path = %path.display(), "using dev database");
        Database::open(&path).expect("failed to open dev database")
    };

    let engine = BlockEngine::new(db).expect("failed to build engine");
    // Headless: no hosts-file writes. A dev server must never edit the real
    // system hosts file as a side effect of a UI click.
    let ctx = Arc::new(AppContext::new_headless(engine));

    let addr = format!("127.0.0.1:{}", cli.port);
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        panic!("cannot bind {addr}: {e}");
    });

    info!(%addr, origin = DEV_ORIGIN, "devserver ready — POST /command");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let ctx = Arc::clone(&ctx);
                std::thread::spawn(move || {
                    if let Err(e) = handle(stream, &ctx) {
                        warn!(error = %e, "request failed");
                    }
                });
            }
            Err(e) => error!(error = %e, "accept failed"),
        }
    }
}

fn handle(mut stream: TcpStream, ctx: &AppContext) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    // Headers — we only care about Content-Length.
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        // Header names are case-insensitive per RFC 9110.
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    match (method.as_str(), path.as_str()) {
        // Preflight for the Vite origin.
        ("OPTIONS", _) => respond(&mut stream, 204, "", None),

        ("GET", "/health") => respond(
            &mut stream,
            200,
            &serde_json::json!({ "ok": true }).to_string(),
            Some("application/json"),
        ),

        ("POST", "/command") => {
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body)?;

            let (status, payload) = run_command(ctx, &body);
            respond(&mut stream, status, &payload, Some("application/json"))
        }

        _ => respond(
            &mut stream,
            404,
            &serde_json::json!({ "error": "not found" }).to_string(),
            Some("application/json"),
        ),
    }
}

/// Deserialise a [`Command`], run it, and render the outcome as JSON.
///
/// Errors come back as `{ "code": ..., "message": ... }` with a non-2xx status,
/// so the frontend can branch on `code` without parsing prose.
fn run_command(ctx: &AppContext, body: &[u8]) -> (u16, String) {
    let cmd: Command = match serde_json::from_slice(body) {
        Ok(c) => c,
        Err(e) => {
            return (
                400,
                serde_json::json!({ "code": "bad_request", "message": e.to_string() }).to_string(),
            );
        }
    };

    match execute(ctx, cmd) {
        Ok(result) => (
            200,
            serde_json::to_string(&result).unwrap_or_else(|e| {
                error!(error = %e, "result failed to serialise");
                r#"{"code":"internal","message":"result serialisation failed"}"#.to_string()
            }),
        ),
        Err(err) => {
            // 4xx for caller mistakes, 500 for genuine backend faults, so
            // browser devtools shows the right colour at a glance.
            let status = match err.exit_code() {
                1 => 500,
                _ => 400,
            };
            (
                status,
                serde_json::to_string(&err)
                    .unwrap_or_else(|_| r#"{"code":"internal","message":"unknown"}"#.to_string()),
            )
        }
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
    content_type: Option<&str>,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };

    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Access-Control-Allow-Origin: {DEV_ORIGIN}\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Content-Length: {len}\r\n",
        len = body.len(),
    );
    if let Some(ct) = content_type {
        head.push_str(&format!("Content-Type: {ct}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");

    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}
