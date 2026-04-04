//! IPC client — sends requests to the Focuser service.
//!
//! Duplicated from focuser-cli to avoid adding tokio to focuser-common.

use anyhow::Result;
use focuser_common::ipc::{IpcRequest, IpcResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Send a request to the Focuser service and return the response.
pub async fn send(request: IpcRequest) -> Result<IpcResponse> {
    #[cfg(windows)]
    {
        send_tcp(request).await
    }
    #[cfg(unix)]
    {
        send_unix(request).await
    }
}

#[cfg(windows)]
async fn send_tcp(request: IpcRequest) -> Result<IpcResponse> {
    let stream = tokio::net::TcpStream::connect("127.0.0.1:17549")
        .await
        .map_err(|_| anyhow::anyhow!("Cannot connect to Focuser service. Is it running?"))?;

    let (reader, mut writer) = stream.into_split();

    let req_json = serde_json::to_string(&request)?;
    writer.write_all(format!("{req_json}\n").as_bytes()).await?;

    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();
    buf_reader.read_line(&mut line).await?;

    let response: IpcResponse = serde_json::from_str(line.trim())?;
    Ok(response)
}

#[cfg(unix)]
async fn send_unix(request: IpcRequest) -> Result<IpcResponse> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = format!("{runtime_dir}/focuser.sock");

    let stream = tokio::net::UnixStream::connect(&path)
        .await
        .map_err(|_| anyhow::anyhow!("Cannot connect to Focuser service. Is it running?"))?;

    let (reader, mut writer) = stream.into_split();

    let req_json = serde_json::to_string(&request)?;
    writer.write_all(format!("{req_json}\n").as_bytes()).await?;

    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();
    buf_reader.read_line(&mut line).await?;

    let response: IpcResponse = serde_json::from_str(line.trim())?;
    Ok(response)
}
