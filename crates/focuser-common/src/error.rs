use thiserror::Error;

#[derive(Debug, Error)]
pub enum FocuserError {
    #[error("Block not found: {0}")]
    BlockNotFound(String),

    #[error("Block list not found: {0}")]
    BlockListNotFound(String),

    #[error("Schedule not found: {0}")]
    ScheduleNotFound(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Service not running")]
    ServiceNotRunning,

    #[error("Service already running")]
    ServiceAlreadyRunning,

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("Lock is active — cannot modify block: {0}")]
    LockActive(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, FocuserError>;
