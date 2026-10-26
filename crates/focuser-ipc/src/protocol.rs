use focuser_app::{Command, CommandErrorPayload, CommandResult};
use focuser_common::{extension::BrowserType, types::BlockList};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const VERSION: u32 = 1;
pub const SERVICE_NAME: &str = "FocuserAuthority";
pub const PRODUCTION_PIPE: &str = r"\\.\pipe\Focuser.Authority.v1";
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
pub const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
pub const MAX_CONNECTIONS: usize = 16;
/// A receiver acknowledges a parsed reply before a byte-mode Windows server
/// disconnects. DisconnectNamedPipe otherwise discards unread output buffers.
pub const REPLY_ACK: u8 = 0x06;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub body: RequestBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RequestBody {
    Command(Box<Command>),
    Browser(BrowserRequest),
    Agent(AgentRequest),
    Native(Box<focuser_common::ipc::IpcRequest>),
    RemovalDecision,
    Maintenance(MaintenanceRequest),
    Update(UpdateRequest),
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub version: u32,
    pub result: Result<ReplyBody, IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ReplyBody {
    Command(CommandResult),
    Browser(BrowserReply),
    Agent(Box<AgentSnapshot>),
    Native(Box<focuser_common::ipc::IpcResponse>),
    Removal(RemovalDecision),
    Maintenance(MaintenanceReply),
    Update(UpdateStatus),
    GraceDeadline(Option<i64>),
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserRequest {
    pub method: String,
    pub path: String,
    pub body: String,
    pub browser: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserReply {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AgentRequest {
    /// Only the enrolled agent may confirm owned browser policies are restored.
    BrowserRestored,
    /// Persist cleanup responsibility before any browser-policy activation.
    BrowserRecoveryIntent,
    BrowserGrace {
        browser: BrowserType,
        missing: bool,
    },
    MaintenanceReady {
        transaction: String,
    },
    /// Separate acknowledgement: an older agent cannot claim complete cleanup.
    CompleteCleanupReady {
        transaction: String,
    },
    Poll {
        after_event: u64,
    },
    Observe {
        foreground: Option<WindowObservation>,
        windows: Vec<WindowObservation>,
        idle: bool,
    },
    ForgetBrowser {
        browser: BrowserType,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowObservation {
    pub pid: u32,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub sequence: u64,
    pub event: AgentFeedback,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentFeedback {
    Pomodoro(focuser_app::command::PomodoroEventDto),
    Allowance(focuser_app::command::AllowanceNotificationDto),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    /// Update exits preserve browser recovery journals, unlike removal cleanup.
    #[serde(default)]
    pub update_shutdown: bool,
    /// Issued only after the authority has consumed removal authorization.
    #[serde(default)]
    pub maintenance: Option<String>,
    /// Only set by the authority for a consumed complete-removal transaction.
    #[serde(default)]
    pub delete_user_data: bool,
    #[serde(default)]
    pub shutdown: bool,
    pub generation: String,
    pub lists: Vec<BlockList>,
    pub active: bool,
    pub enforce_browsers: bool,
    pub grace_secs: u64,
    pub connected: Vec<BrowserType>,
    pub safely_connected: Vec<BrowserType>,
    pub recovery_ready: Vec<BrowserType>,
    pub events: Vec<AgentEvent>,
    pub sound_enabled: bool,
    pub sound_file: Option<String>,
    pub sound_volume: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum UpdateRequest {
    Status {},
    Check {},
    Stage {},
    Activate {
        transaction: String,
    },
    Cancel {
        transaction: String,
    },
    /// Accepted only from the pinned SYSTEM broker process, never the owner UI.
    BrokerBegin {
        transaction: String,
    },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub update_trust: Option<String>,
    pub can_cancel: bool,
    pub available: bool,
    pub version: Option<String>,
    pub transaction: Option<String>,
    pub phase: String,
    pub code: String,
    pub message: String,
    pub downloaded: u64,
    pub total: u64,
    pub runtime_version: String,
    pub security_version: u64,
    pub schema_version: u32,
}

impl UpdateStatus {
    /// Cooperative exit of this client's own process only. This status never
    /// authorizes replacement; the broker must consume its separate permit.
    pub fn shutdown_clients(&self) -> bool {
        matches!(
            self.phase.as_str(),
            "authorized" | "replacing" | "validating" | "rolling_back" | "rollback_validating"
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovalBlocker {
    pub list_id: String,
    pub name: String,
    pub reason: String,
    pub release_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovalDecision {
    /// Only the authority sets this after consuming removal authorization.
    /// Complete removal closes UI/native clients before user-profile cleanup;
    /// the agent has its own later shutdown directive. Allowed alone never exits.
    #[serde(default)]
    pub shutdown_clients: bool,
    pub allowed: bool,
    pub code: String,
    pub blockers: Vec<RemovalBlocker>,
    /// None means at least one commitment needs an authorized unlock.
    pub release_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaintenanceRequest {
    /// Explicit capability boundary: older services must reject full cleanup.
    PrepareCompleteRemoval,
    ConsumeCompleteRemoval {
        permit: String,
    },
    PrepareRemoval {
        delete_data: bool,
    },
    ConsumeRemoval {
        permit: String,
        delete_data: bool,
    },
    RemovalReady {
        transaction: String,
    },
    AbortRemoval {
        transaction: String,
    },
    Upgrade,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceReply {
    pub installation: String,
    pub transaction: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn payload(self) -> CommandErrorPayload {
        CommandErrorPayload {
            code: self.code,
            message: self.message,
        }
    }
}
impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for IpcError {}
impl From<CommandErrorPayload> for IpcError {
    fn from(error: CommandErrorPayload) -> Self {
        Self::new(error.code, error.message)
    }
}

pub async fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<T, IpcError> {
    tokio::time::timeout(IO_TIMEOUT, async {
        let length = stream
            .read_u32_le()
            .await
            .map_err(|e| IpcError::new("malformed", e.to_string()))? as usize;
        if length == 0 || length > MAX_MESSAGE_BYTES {
            return Err(IpcError::new(
                "message_too_large",
                "Invalid IPC frame length",
            ));
        }
        let mut bytes = vec![0; length];
        stream
            .read_exact(&mut bytes)
            .await
            .map_err(|e| IpcError::new("malformed", e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|_| IpcError::new("malformed", "Invalid IPC JSON"))
    })
    .await
    .map_err(|_| IpcError::new("timeout", "IPC read timed out"))?
}

pub async fn write_frame<T: Serialize>(
    stream: &mut (impl AsyncWrite + Unpin),
    value: &T,
) -> Result<(), IpcError> {
    let bytes = serde_json::to_vec(value).map_err(|e| IpcError::new("protocol", e.to_string()))?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(IpcError::new(
            "message_too_large",
            "IPC response exceeds the limit",
        ));
    }
    tokio::time::timeout(IO_TIMEOUT, async {
        stream.write_u32_le(bytes.len() as u32).await?;
        stream.write_all(&bytes).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| IpcError::new("timeout", "IPC write timed out"))?
    .map_err(|e| IpcError::new("transport", e.to_string()))
}

pub fn check_version(request: &Request) -> Result<(), IpcError> {
    if request.version != VERSION {
        Err(IpcError::new(
            "version_mismatch",
            "Unsupported Focuser IPC version",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rejects_oversized_before_allocating_a_body() {
        let (mut a, mut b) = tokio::io::duplex(16);
        a.write_u32_le((MAX_MESSAGE_BYTES + 1) as u32)
            .await
            .unwrap();
        assert_eq!(
            read_frame::<Request>(&mut b).await.unwrap_err().code,
            "message_too_large"
        );
    }
    #[tokio::test]
    async fn rejects_malformed_and_unknown_authority_claims() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let body = br#"{"version":1,"body":{"kind":"ping"},"identity":"SYSTEM","unlocked":true}"#;
        a.write_u32_le(body.len() as u32).await.unwrap();
        a.write_all(body).await.unwrap();
        assert_eq!(
            read_frame::<Request>(&mut b).await.unwrap_err().code,
            "malformed"
        );
        assert_eq!(
            check_version(&Request {
                version: 999,
                body: RequestBody::Ping
            })
            .unwrap_err()
            .code,
            "version_mismatch"
        );
    }
    #[test]
    fn maintenance_does_not_accept_unlock_or_updater_authority_claims() {
        for claim in ["unlocked", "is_updater", "administrator"] {
            let mut request = serde_json::json!({"version":VERSION,"body":{"kind":"maintenance","data":{"operation":"prepare_removal","delete_data":false}}});
            request["body"]["data"][claim] = serde_json::json!(true);
            assert!(serde_json::from_value::<Request>(request).is_err());
        }
    }
    #[test]
    fn update_protocol_accepts_no_paths_commands_keys_or_authority_claims() {
        for claim in [
            "path",
            "command",
            "public_key",
            "unlocked",
            "is_updater",
            "unsigned",
        ] {
            let mut request = serde_json::json!({"version":VERSION,"body":{"kind":"update","data":{"operation":"stage"}}});
            request["body"]["data"][claim] = serde_json::json!("client claim");
            assert!(serde_json::from_value::<Request>(request).is_err());
        }
    }
    #[tokio::test]
    async fn idle_connections_have_a_deadline() {
        let (_a, mut b) = tokio::io::duplex(16);
        assert_eq!(
            read_frame::<Request>(&mut b).await.unwrap_err().code,
            "timeout"
        );
    }
}

#[cfg(test)]
mod update_client_tests {
    use super::*;
    #[test]
    fn only_authorized_replacement_requests_cooperative_client_exit() {
        for phase in [
            "idle",
            "checking",
            "downloading",
            "staged",
            "deferred",
            "complete",
            "rolled_back",
            "unavailable",
        ] {
            assert!(
                !UpdateStatus {
                    phase: phase.into(),
                    ..Default::default()
                }
                .shutdown_clients()
            );
        }
        for phase in [
            "authorized",
            "replacing",
            "validating",
            "rolling_back",
            "rollback_validating",
        ] {
            assert!(
                UpdateStatus {
                    phase: phase.into(),
                    ..Default::default()
                }
                .shutdown_clients()
            );
        }
    }
}
