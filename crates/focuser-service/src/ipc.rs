//! IPC server for receiving commands from CLI/GUI.
//!
//! Uses named pipes on Windows and Unix domain sockets on Linux/macOS.

use focuser_common::error::{FocuserError, Result};
use focuser_common::ipc::{IpcRequest, IpcResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info};

/// The socket/pipe path for IPC.
pub fn ipc_path() -> String {
    #[cfg(windows)]
    {
        r"\\.\pipe\focuser-service".to_string()
    }
    #[cfg(unix)]
    {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{runtime_dir}/focuser.sock")
    }
}

/// Handler function type — called for each incoming IPC request.
pub type RequestHandler = Box<dyn Fn(IpcRequest) -> IpcResponse + Send + Sync>;

/// Start the IPC server. Runs until cancelled.
pub async fn serve(handler: RequestHandler) -> Result<()> {
    let path = ipc_path();
    info!(path = %path, "Starting IPC server");

    #[cfg(unix)]
    {
        serve_unix(&path, handler).await
    }
    #[cfg(windows)]
    {
        serve_tcp(handler).await
    }
}

/// TCP-based IPC for Windows (simpler than named pipes with tokio).
/// Listens on localhost:17549 (arbitrary high port).
#[cfg(windows)]
async fn serve_tcp(handler: RequestHandler) -> Result<()> {
    use std::sync::Arc;

    let handler = Arc::new(handler);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:17549")
        .await
        .map_err(|e| FocuserError::Ipc(format!("Cannot bind TCP: {e}")))?;

    info!("IPC server listening on 127.0.0.1:17549");

    loop {
        let (stream, addr) = listener
            .accept()
            .await
            .map_err(|e| FocuserError::Ipc(format!("Accept failed: {e}")))?;

        debug!(addr = %addr, "IPC client connected");
        let handler = Arc::clone(&handler);

        tokio::spawn(async move {
            if let Err(e) = handle_connection_tcp(stream, &handler).await {
                error!(error = %e, "IPC connection error");
            }
        });
    }
}

#[cfg(windows)]
async fn handle_connection_tcp(
    stream: tokio::net::TcpStream,
    handler: &RequestHandler,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = buf_reader
            .read_line(&mut line)
            .await
            .map_err(|e| FocuserError::Ipc(e.to_string()))?;

        if n == 0 {
            break; // Connection closed
        }

        let request: IpcRequest = match serde_json::from_str(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                let resp = IpcResponse::Error(format!("Invalid request: {e}"));
                let resp_json = serde_json::to_string(&resp).unwrap_or_default();
                writer
                    .write_all(format!("{resp_json}\n").as_bytes())
                    .await
                    .map_err(|e| FocuserError::Ipc(e.to_string()))?;
                continue;
            }
        };

        debug!(request = ?request, "Handling IPC request");
        let response = handler(request);
        let resp_json = serde_json::to_string(&response)?;
        writer
            .write_all(format!("{resp_json}\n").as_bytes())
            .await
            .map_err(|e| FocuserError::Ipc(e.to_string()))?;
    }

    Ok(())
}

/// Unix socket IPC for Linux/macOS.
#[cfg(unix)]
async fn serve_unix(path: &str, handler: RequestHandler) -> Result<()> {
    use std::sync::Arc;

    // Remove stale socket file
    let _ = std::fs::remove_file(path);

    let handler = Arc::new(handler);
    let listener = tokio::net::UnixListener::bind(path)
        .map_err(|e| FocuserError::Ipc(format!("Cannot bind Unix socket: {e}")))?;

    info!(path, "IPC server listening on Unix socket");

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| FocuserError::Ipc(format!("Accept failed: {e}")))?;

        debug!("IPC client connected");
        let handler = Arc::clone(&handler);

        tokio::spawn(async move {
            if let Err(e) = handle_connection_unix(stream, &handler).await {
                error!(error = %e, "IPC connection error");
            }
        });
    }
}

#[cfg(unix)]
async fn handle_connection_unix(
    stream: tokio::net::UnixStream,
    handler: &RequestHandler,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = buf_reader
            .read_line(&mut line)
            .await
            .map_err(|e| FocuserError::Ipc(e.to_string()))?;

        if n == 0 {
            break;
        }

        let request: IpcRequest = match serde_json::from_str(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                let resp = IpcResponse::Error(format!("Invalid request: {e}"));
                let resp_json = serde_json::to_string(&resp).unwrap_or_default();
                writer
                    .write_all(format!("{resp_json}\n").as_bytes())
                    .await
                    .map_err(|e| FocuserError::Ipc(e.to_string()))?;
                continue;
            }
        };

        debug!(request = ?request, "Handling IPC request");
        let response = handler(request);
        let resp_json = serde_json::to_string(&response)?;
        writer
            .write_all(format!("{resp_json}\n").as_bytes())
            .await
            .map_err(|e| FocuserError::Ipc(e.to_string()))?;
    }

    Ok(())
}

