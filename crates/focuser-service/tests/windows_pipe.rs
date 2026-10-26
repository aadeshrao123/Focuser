#![cfg(windows)]
//! Real local Windows pipes, current-user tokens only. No SCM, hosts, browser
//! policies or persistent installation is touched by this harness.
use focuser_ipc::*;
use focuser_service::authority::{Authority, Configuration};
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_pipe_continues_after_clients_disconnect_and_rejects_impostors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scratch.db");
    let (sid, _) = windows::current_identity().unwrap();
    let authority = Arc::new(
        Authority::open(
            &path,
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
    let name = endpoint.pipe_name().unwrap();
    let (sender, receiver) = tokio::sync::watch::channel(false);
    let service = tokio::spawn(focuser_service::server::serve(
        authority.clone(),
        name,
        false,
        receiver,
    ));
    let ui = Client::new(endpoint.clone());
    let ReplyBody::Command(focuser_app::CommandResult::BlockList(list)) = ui
        .request(RequestBody::Command(Box::new(
            focuser_app::Command::CreateBlockList {
                name: "Persistent".into(),
            },
        )))
        .await
        .unwrap()
    else {
        panic!()
    };
    drop(ui);
    let cli = Client::new(endpoint.clone());
    let ReplyBody::Command(focuser_app::CommandResult::BlockLists(lists)) = cli
        .request(RequestBody::Command(Box::new(
            focuser_app::Command::ListBlockLists,
        )))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(lists.iter().any(|l| l.id == list.id));
    cli.request(RequestBody::Command(Box::new(
        focuser_app::Command::EnableProtection {
            list_id: list.id,
            duration_minutes: Some(60),
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            lock: None,
        },
    )))
    .await
    .unwrap();
    assert_eq!(
        cli.request(RequestBody::Command(Box::new(
            focuser_app::Command::DeleteBlockList { id: list.id }
        )))
        .await
        .unwrap_err()
        .code,
        "protected"
    );
    // Real listener framing/version checks, without a writable production DB.
    let pipe_name = endpoint.pipe_name().unwrap();
    let mut raw = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&pipe_name)
        .unwrap();
    write_frame(
        &mut raw,
        &Request {
            version: 999,
            body: RequestBody::Ping,
        },
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let reply: Reply = read_frame(&mut raw).await.unwrap();
    assert_eq!(reply.result.unwrap_err().code, "version_mismatch");
    drop(raw);
    let mut raw = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&pipe_name)
        .unwrap();
    use tokio::io::AsyncWriteExt;
    raw.write_u32_le((MAX_MESSAGE_BYTES + 1) as u32)
        .await
        .unwrap();
    let reply: Reply = read_frame(&mut raw).await.unwrap();
    assert_eq!(reply.result.unwrap_err().code, "message_too_large");
    drop(raw);
    let Endpoint::Development { name, .. } = endpoint else {
        unreachable!()
    };
    let impostor = Client::new(Endpoint::Development {
        name,
        server_pid: std::process::id() + 1,
    });
    assert_eq!(
        impostor.request(RequestBody::Ping).await.unwrap_err().code,
        "server_identity"
    );
    sender.send(true).unwrap();
    service.await.unwrap().unwrap();
}

#[tokio::test]
async fn rejects_a_real_client_token_for_another_configured_owner() {
    let name = format!(r"\\.\pipe\Focuser.Dev.{}", uuid::Uuid::new_v4());
    let (sid, _) = windows::current_identity().unwrap();
    let mut server = windows::create_server(&name, &sid, false, true).unwrap();
    let connect = tokio::spawn(async move {
        use tokio::net::windows::named_pipe::ClientOptions;
        let mut client = ClientOptions::new().open(name).unwrap();
        write_frame(
            &mut client,
            &Request {
                version: VERSION,
                body: RequestBody::Ping,
            },
        )
        .await
        .unwrap();
    });
    server.connect().await.unwrap();
    let _: Request = read_frame(&mut server).await.unwrap();
    assert_eq!(
        windows::authenticate_client(&server, "S-1-5-18")
            .unwrap_err()
            .code,
        "unauthorized"
    );
    connect.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overload_and_early_disconnects_do_not_stop_the_authority_clock() {
    use focuser_common::{
        pomodoro::{PomodoroConfig, PomodoroPhase, PomodoroSession},
        types::BlockList,
    };
    let directory = tempfile::tempdir().unwrap();
    let (sid, _) = windows::current_identity().unwrap();
    let authority = Arc::new(
        Authority::open(
            &directory.path().join("load.db"),
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
    let name = endpoint.pipe_name().unwrap();
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let service = tokio::spawn(focuser_service::server::serve(
        authority.clone(),
        name.clone(),
        false,
        receiver,
    ));
    let open = || async {
        let deadline = tokio::time::Instant::now() + IO_TIMEOUT;
        loop {
            match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
                Ok(pipe) => break pipe,
                Err(error) => {
                    assert!(tokio::time::Instant::now() < deadline, "{error}");
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            }
        }
    };
    let mut held = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        let mut pipe = open().await;
        write_frame(
            &mut pipe,
            &Request {
                version: VERSION,
                body: RequestBody::Ping,
            },
        )
        .await
        .unwrap();
        let reply: Reply = read_frame(&mut pipe).await.unwrap();
        assert!(reply.result.is_ok());
        held.push(pipe); // Intentionally withhold the acknowledgement.
    }
    let list = BlockList::new("Clock under IPC load");
    let mut session = PomodoroSession::new(list.id, PomodoroConfig::CLASSIC, true);
    session.phase_started_at =
        chrono::Utc::now() - chrono::Duration::seconds(i64::from(session.config.work_secs) + 1);
    {
        let engine = authority.context.engine.lock().unwrap();
        engine.db().create_block_list(&list).unwrap();
        engine.db().create_pomodoro_session(&session).unwrap();
    }
    assert_eq!(
        Client::new(endpoint.clone())
            .request(RequestBody::Ping)
            .await
            .unwrap_err()
            .code,
        "busy"
    );
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    assert!(!service.is_finished());
    assert_eq!(
        authority
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_active_pomodoro_session()
            .unwrap()
            .unwrap()
            .current_phase,
        PomodoroPhase::ShortBreak
    );
    drop(held);
    for _ in 0..10 {
        drop(open().await);
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(matches!(
        Client::new(endpoint)
            .request(RequestBody::Ping)
            .await
            .unwrap(),
        ReplyBody::Pong
    ));
    stop.send(true).unwrap();
    service.await.unwrap().unwrap();
}

#[test]
fn disposable_ui_client_process() {
    let Ok(name) = std::env::var("FOCUSER_BASIC_TEST_PIPE") else {
        return;
    };
    let server_pid = std::env::var("FOCUSER_BASIC_TEST_SERVER")
        .unwrap()
        .parse()
        .unwrap();
    let client = Client::new(Endpoint::Development { name, server_pid });
    client.request_blocking(RequestBody::Ping).unwrap();
    std::fs::write(
        std::env::var("FOCUSER_BASIC_TEST_READY").unwrap(),
        b"connected",
    )
    .unwrap();
    // This test child has no UI, database or enforcement. The parent terminates
    // this exact disposable client handle; never an installed Focuser process.
    std::thread::sleep(std::time::Duration::from_secs(30));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminating_a_disposable_ui_client_leaves_service_ticks_rules_and_original_expiry_intact()
{
    use focuser_common::{
        pomodoro::{PomodoroConfig, PomodoroPhase, PomodoroSession},
        types::{BlockList, Protection, WebsiteRule},
    };
    struct TestChild(std::process::Child);
    impl Drop for TestChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("authority.db");
    let (sid, _) = windows::current_identity().unwrap();
    let config = Configuration {
        owner_sid: sid,
        agent_image: std::env::current_exe().unwrap(),
        native_image: std::env::current_exe().unwrap(),
        enforce: false,
    };
    let authority = Arc::new(Authority::open(&db, config.clone()).unwrap());
    let mut list = BlockList::new("Keep enforcing after interface termination");
    list.websites.push(WebsiteRule::domain("distraction.test"));
    list.protection = Some(Protection::for_duration(60));
    let original = list.protection.clone().unwrap();
    {
        let mut engine = authority.context.engine.lock().unwrap();
        engine.db().create_block_list(&list).unwrap();
        engine.refresh().unwrap();
    }
    let name = uuid::Uuid::new_v4().to_string();
    let endpoint = Endpoint::Development {
        name: name.clone(),
        server_pid: std::process::id(),
    };
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let server = tokio::spawn(focuser_service::server::serve(
        authority.clone(),
        endpoint.pipe_name().unwrap(),
        false,
        receiver,
    ));
    let ready = dir.path().join("client-connected");
    let mut child = TestChild(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "disposable_ui_client_process"])
            .env("FOCUSER_BASIC_TEST_PIPE", name)
            .env("FOCUSER_BASIC_TEST_SERVER", std::process::id().to_string())
            .env("FOCUSER_BASIC_TEST_READY", &ready)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = tokio::time::Instant::now() + IO_TIMEOUT;
    while !ready.exists() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "disposable client did not connect"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let client = Client::new(endpoint);
    let response = client
        .request(RequestBody::Command(Box::new(
            focuser_app::Command::CheckDomain {
                domain: "distraction.test".into(),
            },
        )))
        .await
        .unwrap();
    assert!(matches!(
        response,
        ReplyBody::Command(focuser_app::CommandResult::Flag(true))
    ));
    // Force a due transition in scratch storage after the UI client is dead.
    let mut session = PomodoroSession::new(list.id, PomodoroConfig::CLASSIC, true);
    session.phase_started_at =
        chrono::Utc::now() - chrono::Duration::seconds(i64::from(session.config.work_secs) + 1);
    authority
        .context
        .engine
        .lock()
        .unwrap()
        .db()
        .create_pomodoro_session(&session)
        .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let advanced = authority
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_active_pomodoro_session()
            .unwrap()
            .unwrap()
            .current_phase
            == PomodoroPhase::ShortBreak;
        if advanced {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "authority clock stopped with client"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let ReplyBody::Removal(d) = client.request(RequestBody::RemovalDecision).await.unwrap() else {
        panic!()
    };
    assert!(!d.allowed);
    assert_eq!(d.release_at, original.expires_at.map(|t| t.to_rfc3339()));
    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
    drop(client);
    drop(authority);
    let restarted = Authority::open(&db, config).unwrap();
    let engine = restarted.context.engine.lock().unwrap();
    let reloaded = engine
        .db()
        .get_block_list(list.id)
        .unwrap()
        .protection
        .unwrap();
    assert_eq!(reloaded.started_at, original.started_at);
    assert_eq!(reloaded.expires_at, original.expires_at);
    assert!(engine.has_uninstall_protection());
    assert!(engine.check_domain("distraction.test").is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_authority_pipe_is_a_bounded_diagnostic_and_never_authorizes_cleanup() {
    let client = Client::new(Endpoint::Development {
        name: uuid::Uuid::new_v4().to_string(),
        server_pid: std::process::id(),
    });
    let started = std::time::Instant::now();
    let error = client
        .request(RequestBody::Maintenance(
            MaintenanceRequest::PrepareRemoval { delete_data: false },
        ))
        .await
        .unwrap_err();
    assert_eq!(error.code, "service_unavailable");
    assert!(!error.message.is_empty());
    assert!(started.elapsed() < IO_TIMEOUT + std::time::Duration::from_secs(2));
    for _mode in ["interactive", "direct", "silent"] {
        let mut cleanup = 0;
        let failure = focuser_service::removal::perform_removal(
            false,
            |_| Err(error.clone()),
            || Ok(()),
            |_| {
                cleanup += 1;
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(failure.code, "service_unavailable");
        assert_eq!(cleanup, 0);
    }
}
