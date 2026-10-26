//! Keep synchronous command dispatch off Tauri's async executor. In particular,
//! the blocking service client owns a Tokio runtime and cannot run inside one.
use focuser_app::{CommandErrorPayload, CommandResult};

pub async fn execute(
    command: impl FnOnce() -> Result<CommandResult, CommandErrorPayload> + Send + 'static,
) -> Result<CommandResult, CommandErrorPayload> {
    tokio::task::spawn_blocking(command)
        .await
        .unwrap_or_else(|_| {
            Err(CommandErrorPayload {
                code: "internal".into(),
                message: "The desktop command worker failed. Retry the operation.".into(),
            })
        })
}
