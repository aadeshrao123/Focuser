#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod api;
mod audio;
mod browser_guard;
mod browser_recovery;
mod lifecycle;
mod native;
mod observation;
pub static MAINTENANCE: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
use clap::Parser;
use focuser_ipc::*;
#[derive(Parser)]
struct Options {
    /// Explicit orphaned-profile recovery; never starts the normal agent.
    #[arg(long, conflicts_with = "dev")]
    cleanup_orphaned_user_data: bool,
    #[arg(long, requires_all=["dev_pipe", "dev_server_pid", "http_port"])]
    dev: bool,
    #[arg(long, requires = "dev")]
    dev_pipe: Option<String>,
    #[arg(long, requires = "dev")]
    dev_server_pid: Option<u32>,
    #[arg(long, requires = "dev")]
    http_port: Option<u16>,
}
fn extension_settings_url(browser: &str) -> &'static str {
    if browser == "Mozilla Firefox" {
        "about:addons"
    } else {
        "chrome://extensions/"
    }
}
pub fn show_ui() {
    if let Ok(path) = std::env::current_exe()
        && let Some(parent) = path.parent()
    {
        let _ = std::process::Command::new(parent.join("focuser-ui.exe")).spawn();
    }
}
pub fn notify(message: &str) {
    tracing::info!("{message}");
    #[cfg(windows)]
    {
        let message = message.to_owned();
        std::thread::spawn(move || unsafe {
            use ::windows::{Win32::UI::WindowsAndMessaging::*, core::PCWSTR};
            let _ = MessageBoxW(
                None,
                PCWSTR(focuser_ipc::windows::wide(&message).as_ptr()),
                PCWSTR(focuser_ipc::windows::wide("Focuser").as_ptr()),
                MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
            );
        });
    }
}
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let options = Options::parse();
    #[cfg(not(windows))]
    anyhow::ensure!(
        !options.cleanup_orphaned_user_data,
        "Orphaned-profile recovery is Windows-only"
    );
    #[cfg(windows)]
    if options.cleanup_orphaned_user_data {
        let dir = lifecycle::orphaned_installation()?;
        let _singleton = lifecycle::singleton()?
            .ok_or_else(|| anyhow::anyhow!("An enrolled session agent is still running"))?;
        let pending = browser_recovery::pending();
        if pending.is_empty() {
            anyhow::ensure!(
                browser_recovery::orphaned_policies_clear().map_err(anyhow::Error::msg)?,
                "Browser recovery journals are missing and policy values remain. They were preserved; review them with the recovery backup before reporting complete removal."
            );
        }
        for browser in pending {
            browser_recovery::restore(&browser).map_err(anyhow::Error::msg)?;
        }
        anyhow::ensure!(
            browser_recovery::is_clear().map_err(anyhow::Error::msg)?,
            "Browser restoration is incomplete"
        );
        lifecycle::cleanup_user_startup_for(&dir).map_err(anyhow::Error::msg)?;
        lifecycle::cleanup_user_data_for(&dir)?;
        return Ok(());
    }
    let client = Client {
        endpoint: if options.dev {
            Endpoint::Development {
                name: options.dev_pipe.unwrap(),
                server_pid: options.dev_server_pid.unwrap(),
            }
        } else {
            Endpoint::Production
        },
    };
    client.endpoint.pipe_name()?;
    let port = options.http_port.unwrap_or(17549);
    anyhow::ensure!(
        !options.dev || port != 17549,
        "Development must not bind the production browser endpoint"
    );
    #[cfg(windows)]
    anyhow::ensure!(
        windows::current_identity()?.1 != 0,
        "Session agent must run in an interactive user session"
    );
    #[cfg(windows)]
    let _singleton = if !options.dev {
        match lifecycle::singleton()? {
            Some(handle) => Some(handle),
            None => return Ok(()),
        }
    } else {
        None
    };
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
    api::initialize(client.clone());
    let poll = client.clone();
    std::thread::spawn(move || {
        let mut feedback = lifecycle::FeedbackCursor::default();
        loop {
            let result = (|| {
                poll.request_blocking(RequestBody::Agent(observation::sample()))?;
                match poll.request_blocking(RequestBody::Agent(AgentRequest::Poll {
                    after_event: feedback.sequence,
                }))? {
                    ReplyBody::Agent(snapshot) => {
                        if snapshot.update_shutdown && !options.dev {
                            std::process::exit(0);
                        }
                        let initialized = feedback.synchronize(&snapshot.generation);
                        {
                            let mut maintenance = MAINTENANCE.lock().map_err(|_| {
                                IpcError::new("internal", "Agent maintenance lock poisoned")
                            })?;
                            *maintenance = snapshot.maintenance.is_some();
                            if let Some(transaction) = &snapshot.maintenance {
                                if !options.dev {
                                    #[cfg(windows)]
                                    lifecycle::cleanup_user_startup()
                                        .map_err(|e| IpcError::new("cleanup_failed", e))?;
                                    for browser in browser_recovery::pending() {
                                        browser_recovery::restore(&browser)
                                            .map_err(|e| IpcError::new("cleanup_failed", e))?;
                                    }
                                    if !browser_recovery::is_clear()
                                        .map_err(|e| IpcError::new("cleanup_failed", e))?
                                    {
                                        return Err(IpcError::new(
                                            "cleanup_failed",
                                            "Browser recovery journals still require restoration",
                                        ));
                                    }
                                    #[cfg(windows)]
                                    if snapshot.delete_user_data {
                                        lifecycle::cleanup_user_data()?;
                                    }
                                }
                                poll.request_blocking(RequestBody::Agent(
                                    if snapshot.delete_user_data {
                                        AgentRequest::CompleteCleanupReady {
                                            transaction: transaction.clone(),
                                        }
                                    } else {
                                        AgentRequest::MaintenanceReady {
                                            transaction: transaction.clone(),
                                        }
                                    },
                                ))?;
                            }
                        }
                        if snapshot.maintenance.is_some() && snapshot.shutdown && !options.dev {
                            // Expected, service-authorized exit: Task Scheduler
                            // must not treat removal as an agent crash.
                            std::process::exit(0);
                        }
                        for event in &snapshot.events {
                            if initialized && !options.dev {
                                match &event.event {
                                    AgentFeedback::Pomodoro(
                                        focuser_app::command::PomodoroEventDto::PhaseAdvanced {
                                            to,
                                            ..
                                        },
                                    ) => {
                                        audio::phase(&snapshot, to == "work");
                                        notify(&format!("Pomodoro: {to}"));
                                    }
                                    AgentFeedback::Pomodoro(_) => {
                                        notify("Focuser detected a clock change during Pomodoro.")
                                    }
                                    AgentFeedback::Allowance(n) => notify(&format!(
                                        "{}: {} ({} / {} seconds)",
                                        n.target, n.kind, n.used_secs, n.limit_secs
                                    )),
                                }
                            }
                            feedback.sequence = feedback.sequence.max(event.sequence);
                        }
                        // Mark initial synchronization even before the first event.
                        api::update(*snapshot);
                        Ok(())
                    }
                    _ => Err(IpcError::new("protocol", "Unexpected agent reply")),
                }
            })();
            if let Err(error) = result {
                // Poll failures can be transient and happen while no block is
                // active. Keep retrying silently; the desktop health banner
                // reports unavailable authority. A modal notice here would
                // accumulate windows whenever the connection flaps.
                tracing::warn!(%error, "Session agent request failed");
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    // Scratch mode must never modify a user's browser policies or kill apps.
    if !options.dev {
        let guard_client = client.clone();
        std::thread::spawn(move || browser_guard::run(guard_client));
    }
    api::serve(listener, client)?;
    Ok(())
}
