//! Local transport only. A request's claimed fields never determine its Windows
//! identity: the server obtains that from the pipe's impersonation token.
pub mod cleanup;
pub mod protocol;
#[cfg(windows)]
pub mod windows;

pub use protocol::*;

/// Constructed by the Windows transport, never deserialized from a request.
#[derive(Debug, Clone)]
pub struct Peer {
    pub sid: String,
    pub session: u32,
    pub pid: u32,
    pub image: std::path::PathBuf,
}

#[derive(Debug, Clone, Default)]
pub enum Endpoint {
    #[default]
    Production,
    /// Explicit opt-in, never selected as a production fallback. The caller
    /// must pin the process it launched, not accept an advertised server PID.
    Development { name: String, server_pid: u32 },
}

impl Endpoint {
    pub fn pipe_name(&self) -> Result<String, IpcError> {
        match self {
            Self::Production => Ok(PRODUCTION_PIPE.into()),
            Self::Development { name, server_pid } if *server_pid > 4 && valid_dev_name(name) => {
                Ok(format!(r"\\.\pipe\Focuser.Dev.{name}"))
            }
            _ => Err(IpcError::new(
                "invalid_endpoint",
                "Invalid development endpoint or server PID",
            )),
        }
    }
}

pub fn valid_dev_name(name: &str) -> bool {
    (8..=64).contains(&name.len()) && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

#[derive(Debug, Clone, Default)]
pub struct Client {
    pub endpoint: Endpoint,
}

impl Client {
    pub fn new(endpoint: Endpoint) -> Self {
        Self { endpoint }
    }

    pub async fn request(&self, request: RequestBody) -> Result<ReplyBody, IpcError> {
        #[cfg(windows)]
        {
            windows::request(&self.endpoint, request).await
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            Err(IpcError::new(
                "unsupported",
                "The Windows authority is unavailable on this platform",
            ))
        }
    }

    pub fn request_blocking(&self, request: RequestBody) -> Result<ReplyBody, IpcError> {
        // Dedicated callers (Tauri blocking commands, CLI, agent threads), never
        // block an async executor that also has to accept service connections.
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| IpcError::new("transport", e.to_string()))?
            .block_on(self.request(request))
    }

    /// Cooperatively close only this caller's process after committed removal.
    /// Transport errors, an ordinary allowed decision or denials never cause exit.
    pub fn removal_shutdown_requested(&self) -> bool {
        matches!(self.request_blocking(RequestBody::RemovalDecision),
            Ok(ReplyBody::Removal(d)) if d.allowed && d.shutdown_clients)
    }

    pub fn command(
        &self,
        command: focuser_app::Command,
    ) -> Result<focuser_app::CommandResult, focuser_app::CommandErrorPayload> {
        match self.request_blocking(RequestBody::Command(Box::new(command))) {
            Ok(ReplyBody::Command(result)) => Ok(result),
            Ok(_) => Err(IpcError::new("protocol", "Unexpected authority response").payload()),
            Err(error) => Err(error.payload()),
        }
    }
}

pub mod paths;
