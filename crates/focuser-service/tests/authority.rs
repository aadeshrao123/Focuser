use focuser_app::{Command, CommandResult};
use focuser_common::{
    allowance::{Allowance, AllowanceMatch, AllowanceTick},
    types::{AppRule, BlockList, WebsiteRule},
};
use focuser_ipc::*;
use focuser_service::authority::{Authority, Configuration};
use std::path::Path;
fn config() -> Configuration {
    Configuration {
        owner_sid: "S-1-5-21-123".into(),
        agent_image: std::env::current_exe().unwrap(),
        native_image: std::env::current_exe().unwrap(),
        enforce: false,
    }
}
fn open(path: &Path) -> Authority {
    Authority::open(path, config()).unwrap()
}
#[test]
fn grace_deadlines_and_policy_revision_survive_authority_restart_and_reads_do_not_revise_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authority.db");
    let authority = open(&path);
    protect(&authority);
    let peer = Peer {
        sid: config().owner_sid,
        pid: std::process::id(),
        session: 1,
        image: std::env::current_exe().unwrap(),
    };
    let call = |a: &Authority, body| {
        a.handle(
            &peer,
            Request {
                version: VERSION,
                body,
            },
        )
    };
    let request = || {
        RequestBody::Agent(AgentRequest::BrowserGrace {
            browser: focuser_common::extension::BrowserType::Chrome,
            missing: true,
        })
    };
    let ReplyBody::GraceDeadline(first) = call(&authority, request()).unwrap() else {
        panic!()
    };
    assert!(first.is_some());
    let revision = authority
        .context
        .engine
        .lock()
        .unwrap()
        .db()
        .get_setting("_authority.revision")
        .unwrap();
    authority.command(Command::ListBlockLists).unwrap();
    assert_eq!(
        authority
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_setting("_authority.revision")
            .unwrap(),
        revision
    );
    drop(authority);
    let authority = open(&path);
    let ReplyBody::GraceDeadline(second) = call(&authority, request()).unwrap() else {
        panic!()
    };
    assert_eq!(first, second);
    assert_eq!(
        authority
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_setting("_authority.revision")
            .unwrap(),
        revision
    );
    let mut stranger = peer.clone();
    stranger.sid = "S-1-5-21-999".into();
    assert_eq!(
        authority
            .handle(
                &stranger,
                Request {
                    version: VERSION,
                    body: RequestBody::Update(UpdateRequest::Status {})
                }
            )
            .unwrap_err()
            .code,
        "unauthorized"
    );
    assert_eq!(
        call(
            &authority,
            RequestBody::Update(UpdateRequest::BrokerBegin {
                transaction: "client unlocked".into()
            })
        )
        .unwrap_err()
        .code,
        "unauthorized"
    );
    assert_eq!(
        call(&authority, RequestBody::Update(UpdateRequest::Check {}))
            .unwrap_err()
            .code,
        "unsupported"
    );
}
fn protect(authority: &Authority) -> uuid::Uuid {
    let CommandResult::BlockList(list) = authority
        .command(Command::CreateBlockList {
            name: "Commitment".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    authority
        .command(Command::AddWebsiteRule {
            list_id: list.id,
            rule: focuser_common::types::WebsiteMatchType::Domain("focus.test".into()),
        })
        .unwrap();
    authority
        .command(Command::EnableProtection {
            list_id: list.id,
            duration_minutes: Some(60),
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            lock: None,
        })
        .unwrap();
    list.id
}

#[test]
fn browser_enforcement_requires_enabled_web_rules_and_cleanup_ack_is_durable_and_agent_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.db");
    let a = open(&path);
    let peer = Peer {
        sid: config().owner_sid,
        pid: std::process::id(),
        session: 1,
        image: std::env::current_exe().unwrap(),
    };
    let call = |a: &Authority, p: &Peer, r| {
        a.handle(
            p,
            Request {
                version: VERSION,
                body: RequestBody::Agent(r),
            },
        )
    };
    let active =
        |a: &Authority| match call(a, &peer, AgentRequest::Poll { after_event: 0 }).unwrap() {
            ReplyBody::Agent(s) => s.active,
            _ => panic!(),
        };
    assert!(!active(&a));
    let mut list = BlockList::new("No websites yet");
    list.applications.push(AppRule::executable("game.exe"));
    {
        let mut e = a.context.engine.lock().unwrap();
        e.db().create_block_list(&list).unwrap();
        e.refresh().unwrap();
    }
    assert!(!active(&a), "app-only rules must not restrict browsers");
    let mut rule = WebsiteRule::domain("focus.test");
    rule.enabled = false;
    list.websites.push(rule);
    {
        let mut e = a.context.engine.lock().unwrap();
        e.db().update_block_list(&list).unwrap();
        e.refresh().unwrap();
    }
    assert!(
        !active(&a),
        "disabled website rules do not activate enforcement"
    );
    call(&a, &peer, AgentRequest::BrowserRestored).unwrap();
    list.websites[0].enabled = true;
    {
        let mut e = a.context.engine.lock().unwrap();
        e.db().update_block_list(&list).unwrap();
        e.refresh().unwrap();
    }
    assert!(active(&a));
    call(&a, &peer, AgentRequest::BrowserRecoveryIntent).unwrap();
    assert!(call(&a, &peer, AgentRequest::BrowserRestored).is_err());
    a.command(Command::ToggleBlockList {
        id: list.id,
        enabled: false,
    })
    .unwrap();
    assert!(!active(&a));
    assert!(
        call(&a, &peer, AgentRequest::BrowserRecoveryIntent).is_err(),
        "stale snapshots cannot re-activate browser restrictions"
    );
    let mut forged = peer.clone();
    forged.image = "untrusted-ui.exe".into();
    assert_eq!(
        call(&a, &forged, AgentRequest::BrowserRestored)
            .unwrap_err()
            .code,
        "unauthorized"
    );
    call(&a, &peer, AgentRequest::BrowserRestored).unwrap();
    drop(a);
    let a = open(&path);
    assert_eq!(
        a.context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_setting("_authority.browser_cleanup_required")
            .unwrap()
            .as_deref(),
        Some("false")
    );
    assert!(uninstall_decision(&a).allowed);
    assert!(a.can_stop());
}
#[test]
fn clients_get_identical_denials_and_restart_preserves_commitment_and_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authority.db");
    let authority = open(&path);
    let id = protect(&authority);
    let app = Allowance::new(
        AllowanceMatch::AppExecutable("example.exe".into()),
        60,
        true,
    );
    {
        let engine = authority.context.engine.lock().unwrap();
        engine.db().create_allowance(&app).unwrap();
        engine.db().increment_allowance_usage(app.id, 60).unwrap();
    }
    for _client in ["UI", "CLI"] {
        assert_eq!(
            authority
                .command(Command::DeleteBlockList { id })
                .unwrap_err()
                .code,
            "protected"
        );
        assert_eq!(
            authority
                .command(Command::SetSetting {
                    key: "autostart_enabled".into(),
                    value: "0".into()
                })
                .unwrap_err()
                .code,
            "protected"
        );
        assert_eq!(
            authority
                .command(Command::UnlockProtection {
                    list_id: id,
                    response: "UI says unlocked".into()
                })
                .unwrap_err()
                .code,
            "protected"
        );
    }
    assert!(!authority.can_stop());
    drop(authority);
    let restarted = open(&path);
    assert!(!restarted.can_stop());
    assert!(
        restarted
            .context
            .allowance_tracker
            .is_app_blocked("example.exe")
    );
    assert_eq!(
        restarted
            .command(Command::DeleteBlockList { id })
            .unwrap_err()
            .code,
        "protected"
    );
}
#[test]
fn kernel_peer_is_required_and_browser_protocol_cannot_execute_commands() {
    let directory = tempfile::tempdir().unwrap();
    let authority = open(&directory.path().join("authority.db"));
    let mut peer = Peer {
        sid: "S-1-5-21-456".into(),
        session: 1,
        pid: 123,
        image: std::env::current_exe().unwrap(),
    };
    assert_eq!(
        authority
            .handle(
                &peer,
                Request {
                    version: VERSION,
                    body: RequestBody::Ping
                }
            )
            .unwrap_err()
            .code,
        "unauthorized"
    );
    peer.sid = authority.config.owner_sid.clone();
    peer.image = "C:\\untrusted.exe".into();
    assert_eq!(
        authority
            .handle(
                &peer,
                Request {
                    version: VERSION,
                    body: RequestBody::Browser(BrowserRequest {
                        method: "POST".into(),
                        path: "/command".into(),
                        body: "{}".into(),
                        browser: None
                    })
                }
            )
            .unwrap_err()
            .code,
        "unauthorized"
    );
    peer.image = std::env::current_exe().unwrap();
    let reply = authority
        .handle(
            &peer,
            Request {
                version: VERSION,
                body: RequestBody::Browser(BrowserRequest {
                    method: "POST".into(),
                    path: "/command".into(),
                    body: serde_json::to_string(&Command::DeleteAllData).unwrap(),
                    browser: None,
                }),
            },
        )
        .unwrap();
    assert!(matches!(
        reply,
        ReplyBody::Browser(BrowserReply { status: 404, .. })
    ));
    assert!(
        authority
            .handle(
                &peer,
                Request {
                    version: VERSION,
                    body: RequestBody::Native(Box::new(focuser_common::ipc::IpcRequest::Shutdown))
                }
            )
            .is_err()
    );
}
#[test]
fn executable_and_path_rules_continue_without_a_ui_and_absent_agent_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let authority = open(&directory.path().join("authority.db"));
    let mut list = BlockList::new("Continuity");
    list.applications.push(AppRule::executable("game.exe"));
    list.applications
        .push(AppRule::path(r"C:\Blocked\renamed.exe"));
    list.websites.push(WebsiteRule::keyword("distraction"));
    {
        let mut engine = authority.context.engine.lock().unwrap();
        engine.db().create_block_list(&list).unwrap();
        engine.refresh().unwrap();
    }
    authority.tick().unwrap();
    let engine = authority.context.engine.lock().unwrap();
    assert!(authority.should_block_process(&engine, "game.exe", r"C:\Games\game.exe", false));
    assert!(authority.should_block_process(&engine, "chrome.exe", r"C:\Chrome\chrome.exe", false));
    assert!(authority.should_block_process(
        &engine,
        "renamed.exe",
        r"C:\Blocked\renamed.exe",
        false
    ));
    assert!(!authority.should_block_process(&engine, "editor.exe", r"C:\Editor\editor.exe", false));
    assert!(!authority.agent_connected());
}
#[test]
fn foreground_and_process_streams_charge_disjoint_budgets() {
    let db = focuser_core::Database::open_in_memory().unwrap();
    let tracker = focuser_core::allowance::AllowanceTracker::new();
    let strict = Allowance::new(AllowanceMatch::AppExecutable("app.exe".into()), 600, true);
    let loose = Allowance::new(AllowanceMatch::AppExecutable("app.exe".into()), 600, false);
    db.create_allowance(&strict).unwrap();
    db.create_allowance(&loose).unwrap();
    let tick = AllowanceTick {
        hostname: None,
        url: None,
        app_exe: Some("app.exe".into()),
        active: true,
        source: "observer".into(),
        increment_secs: Some(5),
        shared_active: false,
        shared_only: false,
    };
    tracker.ingest_app_tick(&db, &tick, true).unwrap();
    tracker.ingest_app_tick(&db, &tick, false).unwrap();
    assert_eq!(db.get_allowance_used_today(strict.id).unwrap(), 5);
    assert_eq!(db.get_allowance_used_today(loose.id).unwrap(), 5);
}

#[test]
fn pomodoro_transition_does_not_require_a_client() {
    use focuser_common::pomodoro::{PomodoroConfig, PomodoroPhase, PomodoroSession};
    let directory = tempfile::tempdir().unwrap();
    let authority = open(&directory.path().join("authority.db"));
    let list = BlockList::new("Focus");
    let mut session = PomodoroSession::new(list.id, PomodoroConfig::CLASSIC, true);
    session.phase_started_at =
        chrono::Utc::now() - chrono::Duration::seconds(i64::from(session.config.work_secs) + 1);
    {
        let engine = authority.context.engine.lock().unwrap();
        engine.db().create_block_list(&list).unwrap();
        engine.db().create_pomodoro_session(&session).unwrap();
    }
    authority.tick().unwrap();
    let engine = authority.context.engine.lock().unwrap();
    assert_eq!(
        engine
            .db()
            .get_active_pomodoro_session()
            .unwrap()
            .unwrap()
            .current_phase,
        PomodoroPhase::ShortBreak
    );
    assert!(!engine.db().get_block_list(list.id).unwrap().enabled);
}
#[test]
fn removal_decisions_are_service_owned_and_interrupted_removal_reenforces_without_releasing_commitments()
 {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authority.db");
    let authority = open(&path);
    protect(&authority);
    let peer = Peer {
        sid: config().owner_sid,
        session: 1,
        pid: 123,
        image: std::env::current_exe().unwrap(),
    };
    let ReplyBody::Removal(d) = authority
        .handle(
            &peer,
            Request {
                version: VERSION,
                body: RequestBody::RemovalDecision,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(!d.allowed);
    assert_eq!(d.code, "removal_protected");
    // Foreground development endpoints must never gain installer authority.
    assert_eq!(
        authority
            .handle(
                &peer,
                Request {
                    version: VERSION,
                    body: RequestBody::Maintenance(MaintenanceRequest::PrepareRemoval {
                        delete_data: false
                    })
                }
            )
            .unwrap_err()
            .code,
        "unauthorized"
    );
    let list_id = authority.context.engine.lock().unwrap().block_lists()[0].id;
    drop(authority);
    let db = focuser_core::Database::open(&path).unwrap();
    db.set_setting("_authority.maintenance", "interrupted-transaction")
        .unwrap();
    drop(db);
    let restarted = open(&path);
    restarted.tick().unwrap();
    assert!(!restarted.can_stop());
    assert_eq!(
        restarted
            .command(Command::DeleteBlockList { id: list_id })
            .unwrap_err()
            .code,
        "maintenance_in_progress"
    );
    let ReplyBody::Removal(d) = restarted
        .handle(
            &peer,
            Request {
                version: VERSION,
                body: RequestBody::RemovalDecision,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(!d.allowed);
    assert_eq!(d.blockers.len(), 1);
}
#[test]
fn an_agent_acknowledgement_cannot_authorize_removal_and_coverage_is_session_specific() {
    let dir = tempfile::tempdir().unwrap();
    let authority = open(&dir.path().join("authority.db"));
    protect(&authority);
    let mut peer = Peer {
        sid: config().owner_sid,
        session: 1,
        pid: std::process::id(),
        image: std::env::current_exe().unwrap(),
    };
    let call = |peer: &Peer, body| {
        authority.handle(
            peer,
            Request {
                version: VERSION,
                body,
            },
        )
    };
    call(
        &peer,
        RequestBody::Agent(AgentRequest::Observe {
            foreground: None,
            windows: vec![],
            idle: true,
        }),
    )
    .unwrap();
    let status = |peer: &Peer| {
        call(
            peer,
            RequestBody::Command(Box::new(Command::GetSetting {
                key: "_authority.session_agent".into(),
                default: None,
            })),
        )
        .unwrap()
    };
    assert!(
        matches!(status(&peer),ReplyBody::Command(CommandResult::Setting(Some(s))) if s=="connected")
    );
    peer.session = 2;
    assert!(
        matches!(status(&peer),ReplyBody::Command(CommandResult::Setting(Some(s))) if s=="missing")
    );
    peer.session = 1;
    assert_eq!(
        call(
            &peer,
            RequestBody::Agent(AgentRequest::MaintenanceReady {
                transaction: "client says unlocked".into()
            })
        )
        .unwrap_err()
        .code,
        "permit_invalid"
    );
    let ReplyBody::Agent(snapshot) = call(
        &peer,
        RequestBody::Agent(AgentRequest::Poll { after_event: 0 }),
    )
    .unwrap() else {
        panic!()
    };
    assert!(snapshot.maintenance.is_none());
    assert!(!snapshot.shutdown);
    assert!(!authority.can_stop());
}

#[test]
fn ended_browser_coverage_clears_grace_without_an_agent_and_executable_allowances_do_not_extend_it()
{
    let dir = tempfile::tempdir().unwrap();
    let a = open(&dir.path().join("policy.db"));
    {
        let e = a.context.engine.lock().unwrap();
        e.db()
            .set_setting("_authority.browser_grace", "{\"Chrome\":1}")
            .unwrap();
        let allowance = focuser_common::allowance::Allowance::new(
            focuser_common::allowance::AllowanceMatch::AppExecutable("test.exe".into()),
            60,
            false,
        );
        e.db().create_allowance(&allowance).unwrap();
    }
    a.tick().unwrap();
    assert_eq!(
        a.context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_setting("_authority.browser_grace")
            .unwrap()
            .as_deref(),
        Some("{}")
    );
}

fn uninstall_decision(a: &Authority) -> RemovalDecision {
    let peer = Peer {
        sid: config().owner_sid,
        pid: std::process::id(),
        session: 1,
        image: std::env::current_exe().unwrap(),
    };
    match a
        .handle(
            &peer,
            Request {
                version: VERSION,
                body: RequestBody::RemovalDecision,
            },
        )
        .unwrap()
    {
        ReplyBody::Removal(d) => d,
        _ => panic!("expected service-owned removal decision"),
    }
}
#[test]
fn uninstall_only_commitment_survives_pause_short_long_breaks_and_restart_until_verified_unlock() {
    use focuser_common::pomodoro::{PomodoroConfig, PomodoroPhase};
    for cycles in [4, 1] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.db");
        let a = open(&path);
        let CommandResult::BlockList(list) = a
            .command(Command::CreateBlockList {
                name: "Focus commitment".into(),
            })
            .unwrap()
        else {
            panic!()
        };
        a.command(Command::EnableProtection {
            list_id: list.id,
            duration_minutes: Some(60),
            prevent_uninstall: true,
            prevent_service_stop: false,
            prevent_modification: false,
            lock: Some(focuser_app::command::LockSetup::RandomText { length: 6 }),
        })
        .unwrap();
        let stored = a
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap()
            .protection
            .unwrap();
        let mut pomo = PomodoroConfig::CLASSIC;
        pomo.cycles_until_long_break = cycles;
        a.command(Command::PomodoroStart {
            block_list_id: list.id,
            config: pomo,
        })
        .unwrap();
        a.command(Command::PomodoroPause).unwrap();
        assert!(!uninstall_decision(&a).allowed);
        a.command(Command::PomodoroResume).unwrap();
        a.command(Command::PomodoroSkip).unwrap();
        {
            let e = a.context.engine.lock().unwrap();
            let expected = if cycles == 1 {
                PomodoroPhase::LongBreak
            } else {
                PomodoroPhase::ShortBreak
            };
            assert_eq!(
                e.db()
                    .get_active_pomodoro_session()
                    .unwrap()
                    .unwrap()
                    .current_phase,
                expected
            );
            assert!(
                !e.db().get_block_list(list.id).unwrap().enabled,
                "blocking still takes its normal break"
            );
        }
        a.command(Command::PomodoroPause).unwrap();
        drop(a);
        let a = open(&path);
        a.tick().unwrap();
        let d = uninstall_decision(&a);
        assert!(!d.allowed);
        assert!(!d.shutdown_clients);
        assert_eq!(d.release_at, stored.expires_at.map(|t| t.to_rfc3339()));
        let reloaded = a
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap()
            .protection
            .unwrap();
        assert_eq!(stored.started_at, reloaded.started_at);
        assert_eq!(stored.expires_at, reloaded.expires_at);
        // Re-arming or deleting state cannot erase an uninstall-only commitment.
        for command in [Command::EnableProtection { list_id: list.id, duration_minutes: Some(1),
            prevent_uninstall: false, prevent_service_stop: false, prevent_modification: false, lock: None },
            Command::DeleteBlockList { id: list.id }, Command::DeleteAllData,
            Command::ImportConfiguration { json: r#"{"version":1,"app":"Focuser","exported_at":"2026-07-27T00:00:00Z","block_lists":[]}"#.into() }] {
            assert_eq!(a.command(command).unwrap_err().code, "protected");
        }
        assert_eq!(
            a.command(Command::UnlockProtection {
                list_id: list.id,
                response: "UI says unlocked".into()
            })
            .unwrap_err()
            .code,
            "wrong_unlock_response"
        );
        let CommandResult::Text(challenge) = a
            .command(Command::RequestUnlockChallenge { list_id: list.id })
            .unwrap()
        else {
            panic!()
        };
        a.command(Command::UnlockProtection {
            list_id: list.id,
            response: challenge,
        })
        .unwrap();
        let d = uninstall_decision(&a);
        assert!(d.allowed);
        assert!(
            !d.shutdown_clients,
            "permission alone never closes the interface"
        );
        a.command(Command::DeleteBlockList { id: list.id }).unwrap();
    }
}
#[test]
fn expired_stored_commitment_allows_the_existing_removal_transaction_for_each_entry_point() {
    use focuser_common::types::Protection;
    for _mode in ["interactive", "direct", "silent"] {
        for delete_data in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("authority.db");
            let a = open(&path);
            let mut list = BlockList::new("Finished session");
            list.protection = Some(Protection {
                prevent_uninstall: true,
                prevent_service_stop: true,
                prevent_modification: true,
                started_at: chrono::Utc::now() - chrono::Duration::hours(2),
                expires_at: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
            });
            a.context
                .engine
                .lock()
                .unwrap()
                .db()
                .create_block_list(&list)
                .unwrap();
            drop(a);
            let a = open(&path);
            let mut cleanup = 0;
            let mut retention = Vec::new();
            focuser_service::removal::perform_removal(
                delete_data,
                |request| {
                    let phase = match request {
                        MaintenanceRequest::PrepareCompleteRemoval => {
                            assert!(uninstall_decision(&a).allowed);
                            retention.push(true);
                            "prepared"
                        }
                        MaintenanceRequest::ConsumeCompleteRemoval { .. } => {
                            assert!(uninstall_decision(&a).allowed);
                            retention.push(true);
                            "draining"
                        }
                        MaintenanceRequest::PrepareRemoval { delete_data } => {
                            assert!(uninstall_decision(&a).allowed);
                            retention.push(delete_data);
                            "prepared"
                        }
                        MaintenanceRequest::ConsumeRemoval { delete_data, .. } => {
                            assert!(uninstall_decision(&a).allowed);
                            retention.push(delete_data);
                            "draining"
                        }
                        MaintenanceRequest::RemovalReady { .. } => "ready",
                        _ => panic!(),
                    };
                    Ok(MaintenanceReply {
                        installation: "test".into(),
                        transaction: "test-permit".into(),
                        phase: phase.into(),
                    })
                },
                || Ok(()),
                |_| {
                    cleanup += 1;
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(cleanup, 1);
            assert_eq!(retention, vec![delete_data, delete_data]);
        }
    }
}
#[test]
fn protected_stored_commitment_denies_each_entry_point_without_any_cleanup() {
    for _mode in ["interactive", "direct", "silent"] {
        let dir = tempfile::tempdir().unwrap();
        let a = open(&dir.path().join("authority.db"));
        protect(&a);
        let mut cleanup = 0;
        let failure = focuser_service::removal::perform_removal(
            false,
            |_| {
                let d = uninstall_decision(&a);
                assert!(!d.allowed);
                Err(IpcError::new(
                    d.code.clone(),
                    focuser_service::removal::explanation(&d),
                ))
            },
            || Ok(()),
            |_| {
                cleanup += 1;
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(failure.code, "removal_protected");
        assert!(failure.message.contains("Commitment"));
        assert!(failure.message.contains("until"));
        assert_eq!(cleanup, 0);
        assert!(!a.can_stop());
    }
}
