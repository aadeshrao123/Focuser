#![cfg(windows)]
//! Exercise the exact desktop dispatch boundary without launching a GUI or
//! changing the installed service, policies, startup tasks or production data.
#[path = "../../focuser-ui/src/command_worker.rs"]
mod command_worker;

use focuser_app::{Command, CommandResult};
use focuser_ipc::{Client, Endpoint};
use focuser_service::authority::{Authority, Configuration};
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_worker_reads_the_authority_from_an_async_command_handler() {
    let directory = tempfile::tempdir().unwrap();
    let (sid, _) = focuser_ipc::windows::current_identity().unwrap();
    let authority = Arc::new(
        Authority::open(
            &directory.path().join("scratch.db"),
            Configuration {
                owner_sid: sid,
                agent_image: std::env::current_exe().unwrap(),
                native_image: std::env::current_exe().unwrap(),
                enforce: false,
            },
        )
        .unwrap(),
    );
    let endpoint = Endpoint::Development {
        name: uuid::Uuid::new_v4().to_string(),
        server_pid: std::process::id(),
    };
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let server = tokio::spawn(focuser_service::server::serve(
        authority,
        endpoint.pipe_name().unwrap(),
        false,
        receiver,
    ));
    let client = Client::new(endpoint);
    let result = command_worker::execute(move || client.command(Command::ListBlockLists)).await;
    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
    assert!(matches!(result, Ok(CommandResult::BlockLists(lists)) if lists.is_empty()));
}

#[tokio::test]
async fn unavailable_authority_returns_an_error_instead_of_an_unresolved_command() {
    let endpoint = Endpoint::Development {
        name: uuid::Uuid::new_v4().to_string(),
        server_pid: std::process::id(),
    };
    let start = std::time::Instant::now();
    let result =
        command_worker::execute(move || Client::new(endpoint).command(Command::ListBlockLists))
            .await;
    assert_eq!(result.unwrap_err().code, "service_unavailable");
    assert!(start.elapsed() < focuser_ipc::IO_TIMEOUT + std::time::Duration::from_secs(3));
}

#[tokio::test]
async fn worker_failure_is_serializable_instead_of_losing_the_invoke_reply() {
    let result = command_worker::execute(|| panic!("disposable failed desktop command")).await;
    assert_eq!(result.unwrap_err().code, "internal");
}
