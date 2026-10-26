#[cfg(windows)]
mod platform {
    use clap::{Parser, Subcommand};
    use focuser_ipc::*;
    use focuser_service::{
        authority::{Authority, Configuration},
        storage::*,
    };
    use std::{path::PathBuf, sync::Arc};
    use windows_service::{
        define_windows_service,
        service::*,
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    #[derive(Parser)]
    struct Options {
        #[command(subcommand)]
        mode: Mode,
    }
    #[derive(Subcommand)]
    enum Mode {
        /// Explicit scratch mode. Never changes hosts, terminates apps or installs SCM entries.
        Foreground {
            #[arg(long, required = true)]
            dev: bool,
            #[arg(long)]
            db: PathBuf,
            #[arg(long)]
            pipe_name: String,
            #[arg(long)]
            agent_image: PathBuf,
            #[arg(long)]
            native_image: PathBuf,
        },
        /// Elevated installer provisioning; never run on a development host.
        Install {
            #[arg(long)]
            owner_sid: String,
            #[arg(long)]
            source: Option<PathBuf>,
            #[arg(long)]
            initialize_empty: bool,
            #[arg(long)]
            repair: bool,
        },
        /// SYSTEM-only protected broker and boot recovery.
        UpdateBroker {
            #[arg(long)]
            transaction: Option<String>,
        },
        /// Request the next signed update step through the live authority.
        UpdateRequest,
        /// Discover the actual interactive desktop owner for setup (read-only).
        SetupOwner,
        /// Repair identical installed code, or archive only unprovisioned payloads.
        SetupRepair,
        /// Service-authorized cleanup. Silent and interactive callers use the same operation.
        Remove {
            #[arg(long)]
            delete_data: bool,
        },
        /// Inspect the service-owned decision without granting removal authority.
        RemovalDecision,
        /// Read-only compatibility check for the complete-uninstall launcher.
        CleanupCapabilities,
        /// Read-only policy inspection for explicit cold administrator recovery.
        RecoveryInspect {
            #[arg(long)]
            database: PathBuf,
        },
        /// Enter the SCM dispatcher (LocalSystem, installed by an administrator).
        Service,
        /// Explicit elevated provisioning. Does not install/start a service.
        Provision {
            #[arg(long)]
            owner_sid: String,
            #[arg(long)]
            agent_image: PathBuf,
            #[arg(long)]
            native_image: PathBuf,
            #[arg(long)]
            initialize_empty: bool,
        },
        /// Explicit read-only import into an uninitialized provisioned authority.
        Migrate {
            #[arg(long)]
            source: PathBuf,
        },
    }

    fn production() -> anyhow::Result<(Authority, StorageLock)> {
        if windows::current_identity()?.0 != windows::SYSTEM_SID {
            anyhow::bail!("Production authority must run as LocalSystem through SCM");
        }
        // Connect before opening storage: an interrupted/booting kernel guard
        // fences writes until the actual enrolled service restores its policy.
        #[cfg(feature = "kernel-file-guard")]
        let kernel_guard = focuser_service::kernel_guard::connect()?;
        let root = production_root()?;
        focuser_service::update_broker::startup_guard(&root)?;
        for path in [
            &root,
            &root.join("service.json"),
            &root.join("authority.lock"),
            &root.join("authority.db"),
        ] {
            verify_private(path)?;
        }
        let lock = StorageLock::acquire(&root)?;
        let config: ServiceConfiguration =
            serde_json::from_slice(&std::fs::read(root.join("service.json"))?)?;
        if config.version != VERSION {
            anyhow::bail!("Unsupported service configuration");
        }
        verify_code(&std::env::current_exe()?)?;
        verify_code(&config.agent_image)?;
        verify_code(&config.native_image)?;
        let authority = Authority::open(
            &root.join("authority.db"),
            Configuration {
                owner_sid: config.owner_sid,
                agent_image: config.agent_image,
                native_image: config.native_image,
                enforce: true,
            },
        )?;
        #[cfg(feature = "kernel-file-guard")]
        authority.attach_kernel_guard(kernel_guard)?;
        authority.attach_file_guard(focuser_service::file_guard::Guard::new(
            std::env::current_exe()?
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Installed service parent missing"))?
                .to_owned(),
            root.clone(),
        )?)?;
        focuser_service::update_broker::startup_guard(&root)?;
        Ok((authority, lock))
    }

    define_windows_service!(ffi_main, service_main);
    fn service_main(_: Vec<std::ffi::OsString>) {
        if let Err(error) = run_service() {
            tracing::error!(%error, "Service stopped");
        }
    }
    fn run_service() -> anyhow::Result<()> {
        let (authority, _lock) = production()?;
        let authority = Arc::new(authority);
        let (sender, receiver) = tokio::sync::watch::channel(false);
        let control_authority = authority.clone();
        let handle = service_control_handler::register(SERVICE_NAME, move |event| match event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop if control_authority.can_stop() => {
                let _ = sender.send(true);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Shutdown => {
                let _ = sender.send(true);
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
        let status = |state, exit| ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(exit),
            checkpoint: 0,
            wait_hint: std::time::Duration::ZERO,
            process_id: None,
        };
        handle.set_service_status(status(ServiceState::Running, 0))?;
        let runtime = tokio::runtime::Runtime::new()?;
        let result = runtime.block_on(focuser_service::server::serve(
            authority,
            PRODUCTION_PIPE.into(),
            true,
            receiver,
        ));
        handle.set_service_status(status(
            ServiceState::Stopped,
            if result.is_ok() { 0 } else { 1 },
        ))?;
        result.map_err(Into::into)
    }

    pub fn main() -> anyhow::Result<()> {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .init();
        match Options::parse().mode {
            Mode::UpdateRequest => {
                let client = Client::default();
                let ReplyBody::Update(status) =
                    client.request_blocking(RequestBody::Update(UpdateRequest::Status {}))?
                else {
                    anyhow::bail!("Unexpected update status")
                };
                let request = match status.phase.as_str() {
                    "available" => UpdateRequest::Stage {},
                    "staged" | "deferred" => UpdateRequest::Activate {
                        transaction: status
                            .transaction
                            .ok_or_else(|| anyhow::anyhow!("update_missing"))?,
                    },
                    "idle" | "complete" | "rolled_back" | "cancelled" => UpdateRequest::Check {},
                    _ => UpdateRequest::Status {},
                };
                println!(
                    "{}",
                    serde_json::to_string(&client.request_blocking(RequestBody::Update(request))?)?
                );
            }
            Mode::UpdateBroker { transaction } => focuser_service::update_broker::run(transaction)?,
            Mode::Install {
                owner_sid,
                source,
                initialize_empty,
                repair,
            } => {
                if let Err(error) = focuser_service::installation::install(
                    owner_sid,
                    source,
                    initialize_empty,
                    repair,
                ) {
                    anyhow::bail!("{error}");
                }
            }
            Mode::SetupOwner => match focuser_service::setup::desktop_owner() {
                Ok(sid) => println!("{sid}"),
                Err(error) => {
                    anyhow::bail!("{error}");
                }
            },
            Mode::SetupRepair => match focuser_service::setup::repair() {
                Ok(focuser_service::setup::RepairOutcome::Repaired) => {
                    println!("Repair completed. Existing policy and binaries were preserved.")
                }
                Ok(focuser_service::setup::RepairOutcome::Archived(path)) => {
                    println!(
                        "Incomplete payload preserved at {}. Setup can now continue.",
                        path.display()
                    );
                    std::process::exit(10);
                }
                Err(error) => {
                    anyhow::bail!("{error}");
                }
            },
            Mode::CleanupCapabilities => println!("focuser.complete-removal.v1"),
            Mode::RecoveryInspect { database } => println!(
                "{}",
                serde_json::to_string(&focuser_service::recovery::inspect(&database)?)?
            ),
            Mode::Remove { delete_data } => {
                if let Err(error) = focuser_service::installation::remove(delete_data) {
                    // Plain diagnostic text is captured by the NSIS authorization
                    // barrier and is also readable for direct/silent helper use.
                    anyhow::bail!("{error}");
                }
            }
            Mode::RemovalDecision => {
                let response = Client::default().request_blocking(RequestBody::RemovalDecision)?;
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            Mode::Service => service_dispatcher::start(SERVICE_NAME, ffi_main)?,
            Mode::Provision {
                owner_sid,
                agent_image,
                native_image,
                initialize_empty,
            } => {
                verify_code(&agent_image)?;
                verify_code(&native_image)?;
                provision(
                    &ServiceConfiguration {
                        version: VERSION,
                        owner_sid,
                        agent_image,
                        native_image,
                    },
                    initialize_empty,
                )?;
            }
            Mode::Migrate { source } => {
                let root = production_root()?;
                verify_private(&root)?;
                verify_private(&root.join("authority.lock"))?;
                let _lock = StorageLock::acquire(&root)?;
                migrate(&source, &root.join("authority.db"), true)?;
            }
            Mode::Foreground {
                dev,
                db,
                pipe_name,
                agent_image,
                native_image,
            } => {
                anyhow::ensure!(dev, "Foreground mode requires --dev");
                let db = ensure_development_database(&db)?;
                let (owner_sid, _) = windows::current_identity()?;
                let endpoint = Endpoint::Development {
                    name: pipe_name,
                    server_pid: std::process::id(),
                };
                let name = endpoint.pipe_name()?;
                let _lock = StorageLock::acquire(db.parent().unwrap())?;
                let authority = Arc::new(Authority::open(
                    &db,
                    Configuration {
                        owner_sid,
                        agent_image,
                        native_image,
                        enforce: false,
                    },
                )?);
                eprintln!(
                    "Development authority PID={} pipe={name}; enforcement DISABLED",
                    std::process::id()
                );
                tokio::runtime::Runtime::new()?.block_on(async {
                    let (sender, receiver) = tokio::sync::watch::channel(false);
                    tokio::spawn(async move {
                        let _ = tokio::signal::ctrl_c().await;
                        let _ = sender.send(true);
                    });
                    focuser_service::server::serve(authority, name, false, receiver).await
                })?;
            }
        }
        Ok(())
    }
}
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    platform::main()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("focuser-service requires Windows");
    std::process::exit(1);
}
