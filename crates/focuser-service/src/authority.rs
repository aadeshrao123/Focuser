//! One database, one command executor and one policy decision point. This type

//! contains no window, audio device, user-profile lookup or Tauri dependency.

use focuser_app::{AppContext, Command, CommandErrorPayload, CommandResult, SystemSync, execute};

#[cfg(windows)]
use focuser_common::allowance::AllowanceTick;

use focuser_core::{BlockEngine, Database, pomodoro};

use focuser_ipc::*;

use std::collections::{HashMap, VecDeque};

use std::path::{Path, PathBuf};

use std::sync::{Arc, Mutex};

use std::time::{Duration, Instant};

const LEASE: Duration = Duration::from_secs(8);

const MAX_EVENTS: usize = 256;

#[derive(Clone)]
pub struct Configuration {
    pub owner_sid: String,

    pub agent_image: PathBuf,

    pub native_image: PathBuf,

    pub enforce: bool,
}

struct AgentLease {
    pid: u32,

    observed: Instant,

    previous_pid: Option<u32>,

    remainder: Duration,
}

pub struct Authority {
    file_guard: Mutex<Option<crate::file_guard::Guard>>,
    kernel_guard: Mutex<Option<crate::kernel_guard::Controller>>,
    database: PathBuf,

    updates: Option<Arc<Mutex<crate::update::Manager>>>,

    generation: String,

    removal: Mutex<crate::removal::Removal>,

    pub context: Arc<AppContext>,

    pub config: Configuration,

    agents: Arc<Mutex<HashMap<u32, AgentLease>>>,

    clock: Mutex<pomodoro::PomodoroRuntime>,

    events: Mutex<(u64, VecDeque<AgentEvent>)>,

    #[cfg(windows)]
    process_tick: Mutex<Instant>,

    notifications: Mutex<Vec<focuser_app::command::AllowanceNotificationDto>>,
}

struct MachineSync {
    enforce: bool,

    agents: Arc<Mutex<HashMap<u32, AgentLease>>>,
}

impl MachineSync {
    fn observed(&self) -> bool {
        self.agents
            .lock()
            .is_ok_and(|a| a.values().any(|v| v.observed.elapsed() < LEASE))
    }
}

impl SystemSync for MachineSync {
    fn sync_hosts(&self, domains: &[String]) {
        let use_extension =
            self.observed() && !crate::browser_api::get_safely_connected_browsers(120).is_empty();

        if self.enforce
            && let Err(error) = crate::hosts::apply(if use_extension { &[] } else { domains })
        {
            tracing::error!(%error, "Hosts enforcement failed");
        }
    }

    fn hosts_writable(&self) -> bool {
        self.enforce && crate::hosts::writable()
    }

    fn connected_browsers(&self) -> Vec<String> {
        if !self.observed() {
            return Vec::new();
        }

        crate::browser_api::get_connected_browsers(120)
            .iter()
            .map(|b| format!("{b:?}"))
            .collect()
    }

    fn safely_connected_browsers(&self) -> Vec<String> {
        if !self.observed() {
            return Vec::new();
        }

        crate::browser_api::get_safely_connected_browsers(120)
            .iter()
            .map(|b| format!("{b:?}"))
            .collect()
    }

    fn running_browsers(&self) -> Vec<String> {
        focuser_common::process::list()
            .iter()
            .filter_map(|p| focuser_common::browser::identify_browser(&p.name))
            .map(|b| format!("{:?}", b.browser_type))
            .collect()
    }
}

impl Authority {
    fn browser_coverage_active(engine: &BlockEngine) -> Result<bool, IpcError> {
        let db = engine.db();
        if db
            .get_setting_or_default("block_unsupported_browsers", "true")
            .map_err(|e| IpcError::new("storage", e.to_string()))?
            != "true"
        {
            return Ok(false);
        }
        Ok(engine
            .block_lists()
            .iter()
            .any(|l| l.is_effectively_active() && l.websites.iter().any(|r| r.enabled))
            || db
                .list_allowances()
                .map_err(|e| IpcError::new("storage", e.to_string()))?
                .iter()
                .any(|a| {
                    a.enabled
                        && matches!(
                            a.target,
                            focuser_common::allowance::AllowanceMatch::Domain(_)
                        )
                }))
    }
    fn browser_grace(
        &self,

        browser: focuser_common::extension::BrowserType,

        missing: bool,
    ) -> Result<ReplyBody, IpcError> {
        let engine = self
            .context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock"))?;

        let db = engine.db();

        let mut deadlines: HashMap<String, i64> = db
            .get_setting("_authority.browser_grace")
            .map_err(|e| IpcError::new("storage", e.to_string()))?
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| IpcError::new("storage", e.to_string()))?
            .unwrap_or_default();

        let key = format!("{browser:?}");

        let active = Self::browser_coverage_active(&engine)?;

        if missing && active {
            let secs = db
                .get_setting_or_default("extension_grace_period", "60")
                .map_err(|e| IpcError::new("storage", e.to_string()))?
                .parse::<i64>()
                .unwrap_or(60)
                .clamp(0, 86400);

            deadlines
                .entry(key.clone())
                .or_insert_with(|| chrono::Utc::now().timestamp() + secs);
        } else if !active
            || crate::browser_api::get_safely_connected_browsers(120).contains(&browser)
        {
            deadlines.remove(&key);
        }

        db.set_setting(
            "_authority.browser_grace",
            &serde_json::to_string(&deadlines)
                .map_err(|e| IpcError::new("storage", e.to_string()))?,
        )
        .map_err(|e| IpcError::new("storage", e.to_string()))?;

        Ok(ReplyBody::GraceDeadline(deadlines.get(&key).copied()))
    }

    fn browser_cleanup_required(&self) -> Result<bool, IpcError> {
        let engine = self
            .context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock"))?;
        // Old installations may have journals. Unknown is deliberately not clean.
        Ok(engine
            .db()
            .get_setting("_authority.browser_cleanup_required")
            .map_err(|e| IpcError::new("storage", e.to_string()))?
            .as_deref()
            != Some("false"))
    }

    fn ensure_removal_coverage(&self) -> Result<(), IpcError> {
        if !self.agent_connected() && self.browser_cleanup_required()? {
            return Err(IpcError::new(
                "agent_missing",
                "Sign in as the enrolled user and restore agent coverage before removal",
            ));
        }
        Ok(())
    }

    fn revision_changed(&self, removal: &mut crate::removal::Removal) -> Result<(), IpcError> {
        self.begin_kernel_change(true)?;
        removal.changed();

        self.context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock"))?
            .db()
            .set_setting("_authority.revision", &removal.revision.to_string())
            .map_err(|e| IpcError::new("storage", e.to_string()))
    }

    fn begin_kernel_change(&self, policy_write: bool) -> Result<(), IpcError> {
        if let Some(guard) = self
            .file_guard
            .lock()
            .map_err(|_| IpcError::new("internal", "File guard lock"))?
            .as_mut()
        {
            let protected = policy_write
                || guard.status().held_objects != 0
                || !crate::removal::decision(
                    self.context
                        .engine
                        .lock()
                        .map_err(|_| IpcError::new("internal", "Engine lock"))?
                        .block_lists(),
                )
                .allowed;
            if protected {
                guard.fence()?;
            }
        }
        if let Some(guard) = self
            .kernel_guard
            .lock()
            .map_err(|_| IpcError::new("internal", "Kernel guard lock"))?
            .as_mut()
        {
            guard.begin_change()?;
        }
        Ok(())
    }

    fn synchronize_kernel(&self, removal: &crate::removal::Removal) -> Result<(), IpcError> {
        let mut files = self
            .file_guard
            .lock()
            .map_err(|_| IpcError::new("internal", "File guard lock"))?;
        let mut slot = self
            .kernel_guard
            .lock()
            .map_err(|_| IpcError::new("internal", "Kernel guard lock"))?;
        if files.is_some() || slot.is_some() {
            let mut engine = self
                .context
                .engine
                .lock()
                .map_err(|_| IpcError::new("internal", "Engine lock"))?;
            // A failed command may have changed an in-memory list before its
            // database write failed. Only reloadable stored policy may clear a
            // kernel fence; unreadable storage leaves it pending.
            engine
                .refresh()
                .map_err(|e| IpcError::new("storage", e.to_string()))?;
            if let Some(guard) = files.as_mut() {
                guard.synchronize(!crate::removal::decision(engine.block_lists()).allowed)?;
            }
            if let Some(guard) = slot.as_mut() {
                guard.synchronize(
                    &removal.installation,
                    removal.revision,
                    engine.block_lists(),
                )?;
            }
        }
        Ok(())
    }

    /// The SCM lifecycle attaches the verified installation/storage roots. This
    /// is not an IPC operation and clients cannot select protected paths.
    pub fn attach_file_guard(&self, guard: crate::file_guard::Guard) -> Result<(), IpcError> {
        let removal = self
            .removal
            .lock()
            .map_err(|_| IpcError::new("internal", "Maintenance lock"))?;
        let mut slot = self
            .file_guard
            .lock()
            .map_err(|_| IpcError::new("internal", "File guard lock"))?;
        if slot.is_some() {
            return Err(IpcError::new(
                "file_guard_busy",
                "File protection is already attached",
            ));
        }
        *slot = Some(guard);
        drop(slot);
        self.synchronize_kernel(&removal)
    }

    pub fn attach_kernel_guard(
        &self,
        guard: crate::kernel_guard::Controller,
    ) -> Result<(), IpcError> {
        let removal = self
            .removal
            .lock()
            .map_err(|_| IpcError::new("internal", "Maintenance lock"))?;
        let mut slot = self
            .kernel_guard
            .lock()
            .map_err(|_| IpcError::new("internal", "Kernel guard lock"))?;
        if slot.is_some() {
            return Err(IpcError::new(
                "kernel_guard_busy",
                "Controller already attached",
            ));
        }
        *slot = Some(guard);
        drop(slot);
        // Failed initialization retains the kernel's boot fence; no fallback.
        self.synchronize_kernel(&removal)
    }

    fn update_busy(&self) -> bool {
        self.updates
            .as_ref()
            .is_some_and(|m| m.lock().map_or(true, |m| m.busy()))
    }

    fn update_frozen(&self) -> bool {
        self.updates
            .as_ref()
            .is_some_and(|m| m.lock().map_or(true, |m| m.frozen()))
    }

    fn update_paused(&self) -> bool {
        self.updates.as_ref().is_some_and(|m| {
            m.lock().map_or(true, |mut m| {
                // Completion is written by the broker, outside this runtime.
                // Refresh before a newly started agent polls: it must not exit
                // using stale validation state before the next authority tick.
                if m.frozen() {
                    match m.store.load() {
                        Ok(Some(j)) => m.journal = Some(j),
                        _ => return true,
                    }
                }
                m.journal
                    .as_ref()
                    .is_some_and(|j| j.phase.frozen() && j.checkpoint.is_some())
            })
        })
    }
    fn expire_update_authorization(&self) -> Result<(), IpcError> {
        use crate::update::Phase;

        if let Some(manager) = &self.updates {
            let mut m = manager
                .lock()
                .map_err(|_| IpcError::new("internal", "Update lock"))?;

            if m.frozen()
                && let Some(j) = m.store.load()?
            {
                m.journal = Some(j);
            }

            if let Some(mut j) = m.journal.clone() {
                #[cfg(windows)]
                if j.phase.frozen()
                    && j.checkpoint.is_some()
                    && j.origin_generation.as_deref() == Some(self.generation.as_str())
                    && j.previous_version == env!("CARGO_PKG_VERSION")
                    && m.store.floor()? <= j.previous_security
                {
                    let alive = j.broker_pid.is_some_and(|pid| {
                        windows::process_identity(pid).is_ok_and(|p| {
                            p.sid == windows::SYSTEM_SID
                                && self.database.parent().is_some_and(|root| {
                                    same_image(&p.image, &root.join(crate::update::BROKER_NAME))
                                })
                        })
                    });
                    // A tick in this original runtime proves SCM has not finished
                    // stopping it: file replacement is ordered strictly after stop.
                    if !alive {
                        j.phase = Phase::RolledBack;
                        j.code = "broker_lost_before_service_stop".into();
                        m.commit(j.clone())?;
                        if self.config.enforce {
                            let _ = std::thread::Builder::new()
                                .name("focuser-agent-resume".into())
                                .spawn(|| {
                                    let _ = crate::installation::tool(
                                        "schtasks.exe",
                                        &["/Run", "/TN", crate::installation::AGENT_TASK],
                                    );
                                });
                        }
                    }
                }

                if j.phase == Phase::Cancelling && !m.working() {
                    j.phase = Phase::Cancelled;
                    j.code = "cancelled".into();
                    m.commit(j.clone())?;
                }
                if matches!(
                    j.phase,
                    Phase::Checking | Phase::Downloading | Phase::Verifying
                ) && !m.working()
                {
                    j.phase = Phase::RecoveryFault;
                    j.code = "update_worker_interrupted".into();
                    m.commit(j.clone())?;
                }

                if j.phase == Phase::Authorized && chrono::Utc::now().timestamp() >= j.expires_at {
                    j.phase = if j.checkpoint.is_some() {
                        Phase::RecoveryFault
                    } else {
                        Phase::Deferred
                    };

                    j.code = if j.checkpoint.is_some() {
                        "broker_interrupted"
                    } else {
                        "authorization_expired"
                    }
                    .into();

                    m.commit(j)?;
                }
            }
        }

        Ok(())
    }

    fn activation_blocked(&self) -> Result<bool, IpcError> {
        let engine = self
            .context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock"))?;

        let strict = engine.block_lists().iter().any(|l| {
            l.enabled
                && (l.effective_protection().is_some_and(|p| {
                    p.prevent_uninstall || p.prevent_service_stop || p.prevent_modification
                }) || l.scheduled_protection.is_some()
                    || l.lock.is_some())
        });

        let strict_budget = engine
            .db()
            .list_allowances()
            .map_err(|e| IpcError::new("storage", e.to_string()))?
            .iter()
            .any(|a| a.enabled && a.strict_mode);

        let pomodoro = engine
            .db()
            .get_active_pomodoro_session()
            .map_err(|e| IpcError::new("storage", e.to_string()))?
            .is_some();

        Ok(strict || strict_budget || pomodoro)
    }

    fn update_request(
        &self,

        peer: &Peer,

        request: UpdateRequest,

        removal: &mut crate::removal::Removal,
    ) -> Result<UpdateStatus, IpcError> {
        use crate::update::*;

        if matches!(
            request,
            UpdateRequest::Activate { .. } | UpdateRequest::BrokerBegin { .. }
        ) && self
            .kernel_guard
            .lock()
            .map_err(|_| error("internal", "Kernel guard lock"))?
            .is_some()
        {
            return Err(error(
                "kernel_guard_update_deferred",
                "Kernel-protected updates require certified driver/broker coordination; no replacement was started",
            ));
        }

        let broker = matches!(request, UpdateRequest::BrokerBegin { .. });

        #[cfg(windows)]
        let is_broker = self.config.enforce
            && peer.sid == windows::SYSTEM_SID
            && self
                .database
                .parent()
                .is_some_and(|r| same_image(&peer.image, &r.join(BROKER_NAME)));

        #[cfg(not(windows))]
        let is_broker = false;

        if broker && !is_broker
            || peer.sid != self.config.owner_sid
                && !(is_broker
                    && matches!(
                        request,
                        UpdateRequest::Status {} | UpdateRequest::BrokerBegin { .. }
                    ))
        {
            return Err(error(
                "unauthorized",
                "Update operation is outside this Windows identity",
            ));
        }

        if matches!(request, UpdateRequest::Status {}) && self.updates.is_none() {
            return Ok(UpdateStatus {
                phase: "unavailable".into(),

                code: "development_endpoint".into(),

                runtime_version: env!("CARGO_PKG_VERSION").into(),

                security_version: SECURITY_VERSION,

                schema_version: SCHEMA_VERSION,

                ..Default::default()
            });
        }

        let shared = self.updates.as_ref().ok_or_else(|| {
            error(
                "unsupported",
                "Development endpoints cannot activate production updates",
            )
        })?;

        if removal.active() || removal.interrupted {
            return Err(error(
                "maintenance_in_progress",
                "Removal recovery must complete before updates",
            ));
        }

        #[cfg(windows)]
        let _external_lock = if !matches!(
            request,
            UpdateRequest::Status {} | UpdateRequest::BrokerBegin { .. }
        ) {
            Some(crate::storage::StorageLock::maintenance(
                self.database.parent().unwrap(),
            )?)
        } else {
            None
        };

        if matches!(request, UpdateRequest::Check {}) {
            Manager::check(
                shared.clone(),
                removal.installation.clone(),
                removal.revision,
            )?;
        } else if matches!(request, UpdateRequest::Stage {}) {
            Manager::stage(shared.clone())?;
        } else {
            let mut m = shared
                .lock()
                .map_err(|_| error("internal", "Update lock"))?;

            match request {
                UpdateRequest::Status {} => {
                    if let Some(j) = if m.frozen() {
                        m.store.load()?
                    } else {
                        m.journal.clone()
                    } {
                        m.journal = Some(j);
                    }
                }

                UpdateRequest::Cancel { transaction } => {
                    let mut j = m
                        .journal
                        .clone()
                        .ok_or_else(|| error("update_missing", "No transaction"))?;

                    if j.id != transaction || m.frozen() {
                        return Err(error(
                            "transaction_mismatch",
                            "An activated update requires recovery, not cancellation",
                        ));
                    }

                    if j.phase.terminal() {
                        return Err(error(
                            "transaction_replayed",
                            "Transaction already finished",
                        ));
                    }

                    j.phase = if m.working() {
                        Phase::Cancelling
                    } else {
                        Phase::Cancelled
                    };

                    j.code = "cancelled".into();

                    m.commit(j)?;
                }

                UpdateRequest::Activate { transaction } => {
                    let mut j = m
                        .journal
                        .clone()
                        .ok_or_else(|| error("update_missing", "No staged transaction"))?;

                    j.reverify(&Trust::production()?, chrono::Utc::now().timestamp(), false)?;
                    self.tick_state()?;

                    j.authorize(
                        &transaction,
                        &removal.installation,
                        removal.revision,
                        chrono::Utc::now().timestamp(),
                        self.activation_blocked()?,
                    )?;

                    if j.phase == Phase::Authorized {
                        j.origin_generation = Some(self.generation.clone());
                        Trust::production()?.verify(
                            j.envelope
                                .as_ref()
                                .ok_or_else(|| error("package_malformed", "Missing envelope"))?,
                            &j.previous_version,
                            m.store.floor()?,
                            chrono::Utc::now().timestamp(),
                        )?;

                        m.commit(j.clone())?;

                        #[cfg(windows)]
                        {
                            let path = self.database.parent().unwrap().join(BROKER_NAME);

                            crate::update_broker::verify_broker(
                                self.database.parent().unwrap(),
                                &removal.installation,
                            )?;

                            crate::storage::verify_private(&path)?;

                            if let Err(e) = std::process::Command::new(path)
                                .args(["update-broker", "--transaction", &j.id])
                                .spawn()
                            {
                                j.phase = Phase::Deferred;

                                j.code = "broker_start_failed".into();

                                m.commit(j)?;

                                return Err(error("broker_start_failed", e));
                            }
                        }
                    } else {
                        m.commit(j)?;
                    }
                }

                UpdateRequest::BrokerBegin { transaction } => {
                    #[cfg(windows)]
                    crate::storage::verify_private(&peer.image)?;

                    let mut j = m
                        .journal
                        .clone()
                        .ok_or_else(|| error("update_missing", "No activation authorization"))?;

                    if j.id != transaction
                        || j.installation != removal.installation
                        || j.phase != Phase::Authorized
                        || j.origin_generation.as_deref() != Some(self.generation.as_str())
                        || j.checkpoint.is_some()
                        || j.expires_at <= chrono::Utc::now().timestamp()
                    {
                        return Err(error(
                            "transaction_replayed",
                            "Authorization expired, consumed or mismatched",
                        ));
                    }

                    self.tick_state()?;

                    if j.revision != removal.revision || self.activation_blocked()? {
                        j.phase = Phase::Deferred;

                        j.code = "policy_changed".into();

                        m.commit(j)?;

                        return Err(error("policy_changed", "Activation no longer permitted"));
                    }

                    #[cfg(windows)]
                    {
                        j.checkpoint = Some(crate::update_broker::policy_digest(&self.database)?);
                        j.broker_pid = Some(peer.pid);

                        m.commit(j)?;
                    }

                    #[cfg(not(windows))]
                    return Err(error("unsupported", "Windows broker required"));
                }

                _ => unreachable!(),
            }
        }

        shared
            .lock()
            .map(|m| m.status())
            .map_err(|_| error("internal", "Update lock"))
    }

    pub fn open(database: &Path, config: Configuration) -> Result<Self, IpcError> {
        let db = Database::open(database).map_err(|e| IpcError::new("storage", e.to_string()))?;

        let engine = BlockEngine::new(db).map_err(|e| IpcError::new("storage", e.to_string()))?;

        let agents = Arc::new(Mutex::new(HashMap::new()));

        let sync = Arc::new(MachineSync {
            enforce: config.enforce,

            agents: agents.clone(),
        });

        let context = Arc::new(AppContext::new(engine, sync));

        {
            let engine = context
                .engine
                .lock()
                .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

            context
                .allowance_tracker
                .rebuild_from_db(engine.db())
                .map_err(|e| IpcError::new("storage", e.to_string()))?;
        }

        let (installation, interrupted) = {
            let engine = context
                .engine
                .lock()
                .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

            let db = engine.db();

            let installation = db
                .get_setting("_authority.installation")
                .map_err(|e| IpcError::new("storage", e.to_string()))?
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            uuid::Uuid::parse_str(&installation).map_err(|_| {
                IpcError::new(
                    "storage_security",
                    "Invalid installation identity in imported authority metadata",
                )
            })?;
            db.set_setting("_authority.installation", &installation)
                .map_err(|e| IpcError::new("storage", e.to_string()))?;

            let interrupted = db
                .get_setting("_authority.maintenance")
                .map_err(|e| IpcError::new("storage", e.to_string()))?
                .is_some_and(|s| !s.is_empty());

            (installation, interrupted)
        };

        Ok(Self {
            file_guard: Mutex::new(None),
            kernel_guard: Mutex::new(None),
            database: database.to_owned(),

            updates: if config.enforce {
                Some(Arc::new(Mutex::new(crate::update::Manager::open(
                    crate::update::Store::new(
                        database
                            .parent()
                            .ok_or_else(|| {
                                IpcError::new("configuration", "Database parent missing")
                            })?
                            .join("updates"),
                        true,
                    )?,
                )?)))
            } else {
                None
            },

            removal: Mutex::new({
                let mut removal = crate::removal::Removal::new(installation, interrupted);

                removal.revision = context
                    .engine
                    .lock()
                    .map_err(|_| IpcError::new("internal", "Engine lock"))?
                    .db()
                    .get_setting("_authority.revision")
                    .map_err(|e| IpcError::new("storage", e.to_string()))?
                    .map(|s| {
                        s.parse::<u64>().map_err(|_| {
                            IpcError::new("storage_security", "Invalid durable policy revision")
                        })
                    })
                    .transpose()?
                    .unwrap_or(0);

                removal
            }),

            generation: uuid::Uuid::new_v4().to_string(),

            context,

            config,

            agents,

            clock: Mutex::new(Default::default()),

            events: Mutex::new((0, VecDeque::new())),

            #[cfg(windows)]
            process_tick: Mutex::new(Instant::now()),

            notifications: Mutex::new(Vec::new()),
        })
    }

    pub fn handle(&self, peer: &Peer, request: Request) -> Result<ReplyBody, IpcError> {
        check_version(&request)?;

        let mut removal = self
            .removal
            .lock()
            .map_err(|_| IpcError::new("internal", "Maintenance lock poisoned"))?;

        if let RequestBody::Update(update) = request.body {
            return self
                .update_request(peer, update, &mut removal)
                .map(ReplyBody::Update);
        }

        if let RequestBody::Maintenance(request) = request.body {
            if self.update_busy() {
                return Err(IpcError::new(
                    "update_busy",
                    "Cancel or recover the update before removal/repair",
                ));
            }

            return self
                .maintenance(peer, request, &mut removal)
                .map(ReplyBody::Maintenance);
        }

        if peer.sid != self.config.owner_sid {
            return Err(IpcError::new(
                "unauthorized",
                "Not the configured policy owner",
            ));
        }

        if (removal.active() || removal.interrupted || self.update_frozen())
            && !matches!(
                request.body,
                RequestBody::Agent(_) | RequestBody::Ping | RequestBody::RemovalDecision
            )
        {
            return Err(IpcError::new(
                if removal.interrupted {
                    "maintenance_interrupted"
                } else {
                    "maintenance_in_progress"
                },
                "Removal is in progress or was interrupted; policy writes are frozen and recovery is explicit",
            ));
        }

        match request.body {
            RequestBody::Maintenance(_) | RequestBody::Update(_) => unreachable!(),

            RequestBody::RemovalDecision => {
                let complete = removal
                    .transaction
                    .as_ref()
                    .is_some_and(|(_, _, erase)| *erase);
                let mut decision = self.removal_decision_for(complete)?;
                // Complete cleanup needs the UI/native clients to release user
                // profiles after consumption. The agent stays alive until ready.
                decision.shutdown_clients =
                    decision.allowed && removal.active() && (removal.ready || complete);
                Ok(ReplyBody::Removal(decision))
            }

            RequestBody::Ping => Ok(ReplyBody::Pong),

            RequestBody::Command(command) => {
                if policy_mutation(&command) {
                    self.revision_changed(&mut removal)?;
                }

                if matches!(&*command,Command::GetSetting{key,..} if key=="_authority.session_agent")
                {
                    return Ok(ReplyBody::Command(CommandResult::Setting(Some(
                        if self.session_connected(peer.session) {
                            "connected"
                        } else {
                            "missing"
                        }
                        .into(),
                    ))));
                }

                let mutating = policy_mutation(&command);
                let result = self.command_inner(*command);
                if mutating {
                    self.synchronize_kernel(&removal)?;
                }
                let mut result = result?;

                if let CommandResult::BlockingHealth(health) = &mut result {
                    health.app_usage_measurable = self.session_connected(peer.session);
                }

                Ok(ReplyBody::Command(result))
            }

            RequestBody::Agent(request) => {
                self.require_image(peer, &self.config.agent_image)?;

                if let AgentRequest::MaintenanceReady { transaction }
                | AgentRequest::CompleteCleanupReady { transaction } = &request
                {
                    if !removal
                        .transaction
                        .as_ref()
                        .is_some_and(|(id, _, _)| id == transaction)
                    {
                        return Err(IpcError::new(
                            "permit_invalid",
                            "Unknown agent maintenance transaction",
                        ));
                    }
                    let complete = matches!(request, AgentRequest::CompleteCleanupReady { .. });
                    if removal
                        .transaction
                        .as_ref()
                        .is_some_and(|(_, _, erase)| *erase != complete)
                    {
                        return Err(IpcError::new(
                            "cleanup_scope_mismatch",
                            "Complete removal requires the updated agent to acknowledge user-data cleanup",
                        ));
                    }

                    removal.acknowledgements.insert((peer.session, peer.pid));

                    return Ok(ReplyBody::Pong);
                }

                if (removal.active() || self.update_frozen())
                    && matches!(request, AgentRequest::BrowserRecoveryIntent)
                {
                    return Err(IpcError::new(
                        "maintenance_in_progress",
                        "Browser enforcement cannot start during maintenance",
                    ));
                }
                if (removal.active() || self.update_frozen())
                    && !matches!(request, AgentRequest::Poll { .. })
                {
                    return Ok(ReplyBody::Pong);
                }

                if removal.active() {
                    let mut agents = self
                        .agents
                        .lock()
                        .map_err(|_| IpcError::new("internal", "Agent lock poisoned"))?;

                    agents.entry(peer.session).or_insert(AgentLease {
                        pid: peer.pid,

                        observed: Instant::now(),

                        previous_pid: None,

                        remainder: Duration::ZERO,
                    });
                }

                self.agent(
                    peer,
                    request,
                    removal.transaction.as_ref().map(|(id, _, _)| id.clone()),
                    removal.ready,
                    removal
                        .transaction
                        .as_ref()
                        .is_some_and(|(_, _, erase)| *erase),
                )
            }

            RequestBody::Browser(request) => {
                self.require_image(peer, &self.config.agent_image)?;

                if matches!(
                    request.path.as_str(),
                    "/api/add-site" | "/api/remove-site" | "/api/toggle-list"
                ) {
                    self.revision_changed(&mut removal)?;
                }

                // Browser compatibility traffic has a restricted protocol, not

                // access to the full privileged Command enum.

                let reply = crate::browser_api::dispatch(&request, &self.context);
                if matches!(
                    request.path.as_str(),
                    "/api/add-site" | "/api/remove-site" | "/api/toggle-list"
                ) {
                    self.synchronize_kernel(&removal)?;
                }
                Ok(ReplyBody::Browser(reply))
            }

            RequestBody::Native(request) => {
                self.require_image(peer, &self.config.native_image)?;

                self.native(*request)
                    .map(|reply| ReplyBody::Native(Box::new(reply)))
            }
        }
    }

    fn require_image(&self, peer: &Peer, expected: &Path) -> Result<(), IpcError> {
        if peer.session == 0 || !same_image(&peer.image, expected) {
            return Err(IpcError::new(
                "unauthorized",
                "This endpoint requires the configured interactive agent",
            ));
        }

        Ok(())
    }

    pub fn command(&self, command: Command) -> Result<CommandResult, IpcError> {
        let mut removal = self
            .removal
            .lock()
            .map_err(|_| IpcError::new("internal", "Maintenance lock poisoned"))?;

        if removal.active() || removal.interrupted || self.update_frozen() {
            return Err(IpcError::new(
                "maintenance_in_progress",
                "Policy writes are frozen",
            ));
        }

        if policy_mutation(&command) {
            self.revision_changed(&mut removal)?;
        }

        let mutating = policy_mutation(&command);
        let result = self.command_inner(command);
        if mutating {
            self.synchronize_kernel(&removal)?;
        }
        result
    }

    fn command_inner(&self, command: Command) -> Result<CommandResult, IpcError> {
        match &command {
            Command::GetSetting { key, .. } if key == "_authority.file_guard" => {
                let slot = self
                    .file_guard
                    .lock()
                    .map_err(|_| IpcError::new("internal", "File guard lock"))?;
                let status = slot.as_ref().map(|guard| guard.status()).unwrap_or(
                    crate::file_guard::Status {
                        state: "disabled",
                        held_objects: 0,
                        error: None,
                        administrator_resistant: false,
                    },
                );
                return serde_json::to_string(&status)
                    .map(|value| CommandResult::Setting(Some(value)))
                    .map_err(|error| IpcError::new("internal", error.to_string()));
            }
            Command::GetSetting { key, .. } if key == "_authority.kernel_guard" => {
                let slot = self
                    .kernel_guard
                    .lock()
                    .map_err(|_| IpcError::new("internal", "Kernel guard lock"))?;
                return Ok(CommandResult::Setting(Some(
                    slot.as_ref().map_or("disabled", |g| g.status()).into(),
                )));
            }
            Command::AllowanceDrainNotifications => {
                self.flush_notifications();

                return self
                    .notifications
                    .lock()
                    .map(|mut n| CommandResult::AllowanceNotifications(std::mem::take(&mut *n)))
                    .map_err(|_| IpcError::new("internal", "Notification lock poisoned"));
            }

            Command::GetAppIcons { .. } => {
                return Err(IpcError::new(
                    "unsupported",
                    "Application icons must be read with interactive-user privileges",
                ));
            }

            Command::GetSetting { key, .. } if key == "_authority.session_agent" => {
                return Ok(CommandResult::Setting(Some(
                    if self.agent_connected() {
                        "connected"
                    } else {
                        "missing"
                    }
                    .into(),
                )));
            }

            Command::SetSetting { key, .. } if key.starts_with("_authority.") => {
                return Err(IpcError::new(
                    "validation",
                    "Authority metadata is read-only",
                ));
            }

            _ => {}
        }

        let mut result = execute(&self.context, command)
            .map_err(|e| IpcError::from(CommandErrorPayload::from(e)))?;

        if let CommandResult::BlockingHealth(health) = &mut result {
            health.app_usage_measurable = self.agent_connected();
        }

        Ok(result)
    }

    pub fn agent_connected(&self) -> bool {
        self.agents.lock().is_ok_and(|agents| {
            agents
                .values()
                .any(|lease| lease.observed.elapsed() < LEASE)
        })
    }

    fn session_connected(&self, session: u32) -> bool {
        self.agents.lock().is_ok_and(|agents| {
            agents
                .get(&session)
                .is_some_and(|lease| lease.observed.elapsed() < LEASE)
        })
    }

    fn agent(
        &self,

        peer: &Peer,

        request: AgentRequest,

        maintenance: Option<String>,

        shutdown: bool,
        delete_user_data: bool,
    ) -> Result<ReplyBody, IpcError> {
        match request {
            AgentRequest::BrowserRestored | AgentRequest::BrowserRecoveryIntent => {
                let intent = matches!(request, AgentRequest::BrowserRecoveryIntent);
                let engine = self
                    .context
                    .engine
                    .lock()
                    .map_err(|_| IpcError::new("internal", "Engine lock"))?;
                if Self::browser_coverage_active(&engine)? != intent {
                    return Err(IpcError::new(
                        if intent {
                            "coverage_inactive"
                        } else {
                            "coverage_active"
                        },
                        if intent {
                            "No browser blocking is currently running"
                        } else {
                            "Browser cleanup can be confirmed only after blocking coverage ends"
                        },
                    ));
                }
                let value = if intent { "true" } else { "false" };
                if engine
                    .db()
                    .get_setting("_authority.browser_cleanup_required")
                    .map_err(|e| IpcError::new("storage", e.to_string()))?
                    .as_deref()
                    != Some(value)
                {
                    engine
                        .db()
                        .set_setting("_authority.browser_cleanup_required", value)
                        .map_err(|e| IpcError::new("storage", e.to_string()))?;
                }
                Ok(ReplyBody::Pong)
            }
            AgentRequest::BrowserGrace { browser, missing } => self.browser_grace(browser, missing),

            AgentRequest::MaintenanceReady { .. } | AgentRequest::CompleteCleanupReady { .. } => {
                unreachable!()
            }

            AgentRequest::Poll { after_event } => {
                let update_shutdown = self.update_paused();

                let engine = self
                    .context
                    .engine
                    .lock()
                    .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

                let db = engine.db();

                let setting = |key, default| {
                    db.get_setting_or_default(key, default)
                        .unwrap_or_else(|_| default.into())
                };

                let events = self
                    .events
                    .lock()
                    .map_err(|_| IpcError::new("internal", "Event lock poisoned"))?
                    .1
                    .iter()
                    .filter(|e| e.sequence > after_event)
                    .cloned()
                    .collect();

                let active = Self::browser_coverage_active(&engine)?;
                if active {
                    // Persist before granting the agent an active snapshot: a
                    // crash after applying browser policies must require cleanup.
                    db.set_setting("_authority.browser_cleanup_required", "true")
                        .map_err(|e| IpcError::new("storage", e.to_string()))?;
                }
                let snapshot = AgentSnapshot {
                    delete_user_data,
                    update_shutdown,

                    maintenance,

                    shutdown,

                    generation: self.generation.clone(),

                    lists: engine.block_lists().to_vec(),

                    active,

                    enforce_browsers: setting("block_unsupported_browsers", "true") == "true",

                    grace_secs: setting("extension_grace_period", "60")
                        .parse()
                        .unwrap_or(60),

                    connected: crate::browser_api::get_connected_browsers(120)
                        .into_iter()
                        .collect(),

                    safely_connected: crate::browser_api::get_safely_connected_browsers(120)
                        .into_iter()
                        .collect(),

                    recovery_ready: crate::browser_api::get_recovery_ready_browsers()
                        .into_iter()
                        .collect(),

                    events,

                    sound_enabled: setting("pomodoro_sound", "false") == "true",

                    sound_file: db
                        .get_setting("pomodoro_sound_file")
                        .ok()
                        .flatten()
                        .filter(|s| !s.is_empty()),

                    sound_volume: setting("pomodoro_sound_volume", "70")
                        .parse::<u8>()
                        .unwrap_or(70)
                        .min(100),
                };

                Ok(ReplyBody::Agent(Box::new(snapshot)))
            }

            AgentRequest::ForgetBrowser { browser } => {
                crate::browser_api::forget_browser(&browser);

                Ok(ReplyBody::Pong)
            }

            AgentRequest::Observe {
                foreground,

                windows,

                idle,
            } => {
                if windows.len() > 256
                    || windows
                        .iter()
                        .chain(foreground.iter())
                        .any(|w| w.title.len() > 4096)
                {
                    return Err(IpcError::new("validation", "Observation exceeds the limit"));
                }

                #[cfg(windows)]
                let foreground_process = foreground
                    .as_ref()
                    .and_then(|w| focuser_ipc::windows::process_identity(w.pid).ok());

                #[cfg(windows)]
                if foreground_process
                    .as_ref()
                    .is_some_and(|p| p.sid != peer.sid || p.session != peer.session)
                {
                    return Err(IpcError::new(
                        "unauthorized",
                        "Foreground process is outside this session",
                    ));
                }

                let elapsed = {
                    let mut agents = self
                        .agents
                        .lock()
                        .map_err(|_| IpcError::new("internal", "Agent lock poisoned"))?;

                    let previous = agents.get(&peer.session).filter(|a| a.pid == peer.pid);

                    let measured = previous
                        .map(|a| a.observed.elapsed() + a.remainder)
                        .unwrap_or_default();

                    let seconds = measured.as_secs().min(5) as u32;

                    let remainder = Duration::from_nanos(u64::from(measured.subsec_nanos()));

                    let stable = previous
                        .is_some_and(|a| a.previous_pid == foreground.as_ref().map(|f| f.pid));

                    if agents.len() >= 16 && !agents.contains_key(&peer.session) {
                        agents.retain(|_, a| a.observed.elapsed() < LEASE);

                        if agents.len() >= 16 {
                            return Err(IpcError::new("busy", "Too many agent sessions"));
                        }
                    }

                    agents.insert(
                        peer.session,
                        AgentLease {
                            pid: peer.pid,

                            observed: Instant::now(),

                            previous_pid: foreground.as_ref().map(|f| f.pid),

                            remainder: if idle { Duration::ZERO } else { remainder },
                        },
                    );

                    (seconds, stable)
                };

                #[cfg(windows)]
                {
                    for window in &windows {
                        self.observe_window(peer, window);
                    }

                    if !idle
                        && elapsed.0 > 0
                        && let Some(process) = foreground_process
                    {
                        let exe = process
                            .image
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();

                        let tick = AllowanceTick {
                            hostname: None,

                            url: None,

                            app_exe: Some(exe),

                            active: true,

                            source: format!("session:{}", peer.session),

                            increment_secs: Some(elapsed.0),

                            shared_active: elapsed.1,

                            shared_only: false,
                        };

                        let engine = self
                            .context
                            .engine
                            .lock()
                            .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

                        self.context
                            .allowance_tracker
                            .ingest_app_tick(engine.db(), &tick, true)
                            .map_err(|e| IpcError::new("storage", e.to_string()))?;
                    }
                }

                #[cfg(not(windows))]
                let _ = (elapsed, foreground, idle);

                Ok(ReplyBody::Pong)
            }
        }
    }

    #[cfg(windows)]
    fn observe_window(&self, caller: &Peer, window: &WindowObservation) {
        if !self.config.enforce {
            return;
        }

        let _ = focuser_ipc::windows::enforce_process(
            window.pid,
            &caller.sid,
            Some(caller.session),
            |process| {
                let Ok(engine) = self.context.engine.lock() else {
                    return false;
                };

                let name = process
                    .image
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();

                let path = process.image.to_string_lossy();

                self.evaluate_process(&engine, &name, &path, Some(&window.title), true)
            },
        );
    }

    /// Wall-clock/state transitions run independently of every client connection.
    pub fn tick(&self) -> Result<(), IpcError> {
        let mut removal = self
            .removal
            .lock()
            .map_err(|_| IpcError::new("internal", "Maintenance lock poisoned"))?;

        self.expire_update_authorization()?;

        if self.update_paused() {
            return Ok(());
        }
        if removal.active() {
            #[cfg(not(windows))]
            let caller_alive = true;

            #[cfg(windows)]
            let caller_alive = removal.transaction.as_ref().is_some_and(|(_, caller, _)| {
                windows::process_identity(caller.pid())
                    .is_ok_and(|peer| crate::removal::Caller::from(&peer) == *caller)
            });

            if !removal.resume_if_abandoned(Instant::now(), caller_alive) {
                return Ok(());
            }
        }

        let before = self.policy_bytes()?;

        let state = self
            .begin_kernel_change(false)
            .and_then(|_| self.tick_state());

        // Persistence/hosts failures must not skip executable enforcement from

        // the last validated policy cache. Storage errors are still reported.

        #[cfg(windows)]
        self.enforce_processes();

        self.flush_notifications();

        if self.policy_bytes()? != before {
            self.revision_changed(&mut removal)?;
        }

        self.synchronize_kernel(&removal)?;

        state
    }

    fn tick_state(&self) -> Result<(), IpcError> {
        let mut engine = self
            .context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

        engine
            .refresh()
            .map_err(|e| IpcError::new("storage", e.to_string()))?;

        let mut clock = self
            .clock
            .lock()
            .map_err(|_| IpcError::new("internal", "Clock lock poisoned"))?;

        match pomodoro::tick(&mut engine, &mut clock)
            .map_err(|e| IpcError::new("storage", e.to_string()))?
        {
            pomodoro::TickOutcome::PhaseAdvanced { to, cycle, .. } => {
                self.context
                    .push_pomodoro_event(focuser_app::PomodoroEvent::PhaseAdvanced {
                        to: to.as_str().into(),

                        cycle,
                    });

                self.event(focuser_app::command::PomodoroEventDto::PhaseAdvanced {
                    to: to.as_str().into(),

                    cycle,
                });
            }

            pomodoro::TickOutcome::TamperDetected => {
                self.context
                    .push_pomodoro_event(focuser_app::PomodoroEvent::TamperDetected);

                self.event(focuser_app::command::PomodoroEventDto::TamperDetected);
            }

            _ => {}
        }

        drop(clock);

        // Preserve deadlines over agent/service restarts, but retain the existing
        // grace reset when browser coverage ends, even when no agent is present.
        if !Self::browser_coverage_active(&engine)?
            && engine
                .db()
                .get_setting("_authority.browser_grace")
                .map_err(|e| IpcError::new("storage", e.to_string()))?
                .is_some_and(|value| value != "{}")
        {
            engine
                .db()
                .set_setting("_authority.browser_grace", "{}")
                .map_err(|e| IpcError::new("storage", e.to_string()))?;
        }
        let mut domains = engine.collect_blocked_domains();

        domains.extend(self.context.allowance_tracker.blocked_domains());

        let exceptions = self.context.allowance_exempt_domains(&engine);

        domains.retain(|d| !focuser_common::host::any_host_matches(&exceptions, d));

        drop(engine);

        if self.agent_connected()
            && !crate::browser_api::get_safely_connected_browsers(120).is_empty()
        {
            domains.clear();
        }

        if self.config.enforce {
            crate::hosts::apply(&domains).map_err(|e| IpcError::new("enforcement", e))
        } else {
            Ok(())
        }
    }

    fn event(&self, event: focuser_app::command::PomodoroEventDto) {
        self.feedback(AgentFeedback::Pomodoro(event));
    }

    fn flush_notifications(&self) {
        if let Ok(mut pending) = self.notifications.lock() {
            for event in self.context.allowance_tracker.take_notifications() {
                let dto = focuser_app::command::AllowanceNotificationDto {
                    allowance_id: event.allowance_id,

                    target: event.target,

                    kind: format!("{:?}", event.kind).to_lowercase(),

                    used_secs: event.used_secs,

                    limit_secs: event.limit_secs,
                };

                self.feedback(AgentFeedback::Allowance(dto.clone()));

                if pending.len() >= MAX_EVENTS {
                    pending.remove(0);
                }

                pending.push(dto);
            }
        }
    }

    fn feedback(&self, event: AgentFeedback) {
        if let Ok(mut events) = self.events.lock() {
            events.0 += 1;

            let sequence = events.0;

            events.1.push_back(AgentEvent { sequence, event });

            while events.1.len() > MAX_EVENTS {
                events.1.pop_front();
            }
        }
    }

    #[cfg(windows)]
    fn enforce_processes(&self) {
        if !self.config.enforce {
            return;
        }

        let increment = self
            .process_tick
            .lock()
            .map(|mut previous| {
                let elapsed = previous.elapsed();

                let seconds = elapsed.as_secs().min(120) as u32;

                if elapsed.as_secs() > 120 {
                    *previous = Instant::now();
                } else {
                    *previous += Duration::from_secs(u64::from(seconds));
                }

                seconds
            })
            .unwrap_or(1);

        let owned: Vec<_> = focuser_common::process::list()
            .into_iter()
            .filter(|p| p.is_killable())
            .filter_map(|p| focuser_ipc::windows::process_identity(p.pid).ok())
            .filter(|p| p.sid == self.config.owner_sid && p.session != 0)
            .collect();

        let mut open_executables = std::collections::HashSet::new();

        for process in owned {
            let name = process
                .image
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let path = process.image.to_string_lossy();

            open_executables.insert(name.to_ascii_lowercase());

            let connected = self.session_connected(process.session);

            let blocked = {
                let Ok(engine) = self.context.engine.lock() else {
                    continue;
                };

                self.should_block_process(&engine, &name, &path, connected)
            };

            if blocked {
                let killed = focuser_ipc::windows::enforce_process(
                    process.pid,
                    &self.config.owner_sid,
                    Some(process.session),
                    |actual| actual.image == process.image,
                )
                .unwrap_or(false);

                if killed && let Ok(engine) = self.context.engine.lock() {
                    let _ = engine.record_blocked(&name);
                }
            }
        }

        // Loose application allowances count any open executable once per tick;

        // strict allowances are charged only by authenticated foreground samples.

        if let Ok(engine) = self.context.engine.lock() {
            for exe in open_executables {
                if increment == 0 {
                    break;
                }

                let tick = AllowanceTick {
                    hostname: None,

                    url: None,

                    app_exe: Some(exe),

                    active: false,

                    source: "authority-processes".into(),

                    increment_secs: Some(increment),

                    shared_active: false,

                    shared_only: false,
                };

                let _ = self
                    .context
                    .allowance_tracker
                    .ingest_app_tick(engine.db(), &tick, false);
            }
        }
    }

    pub fn should_block_process(
        &self,

        engine: &BlockEngine,

        name: &str,

        path: &str,

        agent: bool,
    ) -> bool {
        self.evaluate_process(engine, name, path, None, agent)
    }

    fn evaluate_process(
        &self,

        engine: &BlockEngine,

        name: &str,

        path: &str,

        title: Option<&str>,

        agent: bool,
    ) -> bool {
        let active_web = engine
            .block_lists()
            .iter()
            .any(|l| l.is_effectively_active() && l.websites.iter().any(|r| r.enabled))
            || engine.db().list_allowances().is_ok_and(|items| {
                items.iter().any(|a| {
                    a.enabled
                        && matches!(
                            a.target,
                            focuser_common::allowance::AllowanceMatch::Domain(_)
                        )
                })
            });

        // Advanced website rules and strict quotas cannot be observed in Session

        // 0. Missing agent coverage is explicit and browser access fails closed.

        if !agent && active_web && focuser_common::browser::identify_browser(name).is_some() {
            return true;
        }

        if !agent && engine.db().list_allowances().is_ok_and(|items|items.iter().any(|a|a.enabled && a.strict_mode && matches!(&a.target,focuser_common::allowance::AllowanceMatch::AppExecutable(exe) if exe.eq_ignore_ascii_case(name)))) { return true; }

        if self.context.allowance_tracker.is_app_blocked(name) {
            return true;
        }

        let allowances = self.context.allowance_tracker.active_allowance_apps(engine);

        engine.block_lists().iter().any(|list| {
            if !list.should_block_app(name, Some(path), title) {
                return false;
            }

            let shared = agent
                && list.shared_allowance.is_some()
                && engine.db().shared_permits_at(list, chrono::Local::now());

            let hours = list
                .schedule
                .as_ref()
                .is_some_and(|s| !s.time_slots.is_empty());

            let individual =
                agent && !hours && allowances.iter().any(|a| a.eq_ignore_ascii_case(name));

            !shared && !individual
        })
    }

    pub fn can_stop(&self) -> bool {
        let Ok(removal) = self.removal.lock() else {
            return false;
        };

        if removal.active() {
            return removal.ready;
        }

        if self.update_paused() {
            return true;
        }

        self.context
            .engine
            .lock()
            .is_ok_and(|engine| !engine.has_service_protection())
    }

    fn policy_bytes(&self) -> Result<Vec<u8>, IpcError> {
        let engine = self
            .context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

        serde_json::to_vec(engine.block_lists())
            .map_err(|e| IpcError::new("storage", e.to_string()))
    }

    fn removal_decision(&self) -> Result<RemovalDecision, IpcError> {
        self.removal_decision_for(false)
    }
    fn removal_decision_for(&self, complete: bool) -> Result<RemovalDecision, IpcError> {
        let mut engine = self
            .context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

        engine
            .refresh()
            .map_err(|e| IpcError::new("storage", e.to_string()))?;

        let mut decision = if complete {
            crate::removal::complete_decision_at(engine.block_lists(), chrono::Local::now())
        } else {
            crate::removal::decision(engine.block_lists())
        };
        if complete {
            if let Some(session) = engine
                .db()
                .get_active_pomodoro_session()
                .map_err(|e| IpcError::new("storage", e.to_string()))?
            {
                let id = session.block_list_id.to_string();
                // Keep a separate session release condition if a timed lock is
                // also present: its expiry alone does not end a Pomodoro.
                decision
                    .blockers
                    .retain(|b| b.list_id != id || b.reason != "running_block");
                {
                    decision.blockers.push(RemovalBlocker {
                        list_id: id,
                        name: engine
                            .block_lists()
                            .iter()
                            .find(|l| l.id == session.block_list_id)
                            .map_or_else(|| "Pomodoro session".into(), |l| l.name.clone()),
                        reason: "pomodoro_session".into(),
                        release_at: None,
                    });
                }
                decision.allowed = false;
                decision.code = "removal_protected".into();
                decision.release_at = None;
            }
        }
        Ok(decision)
    }

    fn journal_removal(&self, value: &str) -> Result<(), IpcError> {
        let engine = self
            .context
            .engine
            .lock()
            .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

        engine
            .db()
            .set_setting("_authority.maintenance", value)
            .map_err(|e| IpcError::new("storage", e.to_string()))
    }

    fn maintenance(
        &self,

        peer: &Peer,

        request: MaintenanceRequest,

        removal: &mut crate::removal::Removal,
    ) -> Result<MaintenanceReply, IpcError> {
        // Requests cannot name a trusted identity. Only our protected installed

        // executable running with an actual elevated administrator token may

        // coordinate removal. Elevation still cannot override policy.

        #[cfg(windows)]
        {
            if !self.config.enforce
                || !same_image(
                    &peer.image,
                    &std::env::current_exe()
                        .map_err(|e| IpcError::new("identity", e.to_string()))?,
                )
            {
                return Err(IpcError::new(
                    "unauthorized",
                    "Installed maintenance helper required",
                ));
            }

            crate::storage::verify_code(&peer.image)?;

            windows::require_elevated_process(peer.pid)?;
        }

        #[cfg(not(windows))]
        {
            let _ = (peer, request, removal);

            Err(IpcError::new("unsupported", "Windows maintenance required"))
        }

        #[cfg(windows)]
        {
            let caller = crate::removal::Caller::from(peer);
            // New operations fail closed against older services. All data-erasing
            // paths (including older clients) use the same stronger decision.
            let request = match request {
                MaintenanceRequest::PrepareCompleteRemoval => {
                    MaintenanceRequest::PrepareRemoval { delete_data: true }
                }
                MaintenanceRequest::ConsumeCompleteRemoval { permit } => {
                    MaintenanceRequest::ConsumeRemoval {
                        permit,
                        delete_data: true,
                    }
                }
                request => request,
            };

            match request {
                MaintenanceRequest::Upgrade => Err(IpcError::new(
                    "upgrade_deferred",
                    "Secure binary-update coordination is not implemented; no files may be replaced",
                )),

                MaintenanceRequest::PrepareRemoval { delete_data } => {
                    if removal.interrupted {
                        return Err(IpcError::new(
                            "maintenance_interrupted",
                            "Use documented administrator recovery before retrying removal",
                        ));
                    }

                    let d = self.removal_decision_for(delete_data)?;

                    if !d.allowed {
                        return Err(IpcError::new(
                            d.code.clone(),
                            crate::removal::explanation(&d),
                        ));
                    }

                    self.ensure_removal_coverage()?;
                    if delete_data && !self.agent_connected() {
                        return Err(IpcError::new(
                            "agent_missing",
                            "Complete removal needs the enrolled user's session agent to remove user data safely. Sign in as that user and retry.",
                        ));
                    }

                    removal.issue(caller, delete_data, Instant::now())
                }

                MaintenanceRequest::ConsumeRemoval {
                    permit,

                    delete_data,
                } => {
                    // Every production mutation and tick takes this same lock.

                    // Refresh and re-evaluate wall-clock boundaries as well.

                    removal.consume(&caller, &permit, delete_data, Instant::now())?;

                    let d = self.removal_decision_for(delete_data)?;

                    if !d.allowed {
                        removal.transaction = None;

                        return Err(IpcError::new(
                            d.code.clone(),
                            crate::removal::explanation(&d),
                        ));
                    }

                    self.synchronize_kernel(removal)?;
                    if let Some(guard) = self
                        .kernel_guard
                        .lock()
                        .map_err(|_| IpcError::new("internal", "Kernel guard lock"))?
                        .as_mut()
                    {
                        if let Err(error) = guard.authorize_removal(&permit, removal.revision) {
                            removal.transaction = None;
                            return Err(error);
                        }
                    }

                    if let Err(e) = self.journal_removal(&permit) {
                        removal.transaction = None;

                        return Err(e);
                    }

                    Ok(removal.reply(permit, "draining"))
                }

                MaintenanceRequest::RemovalReady { transaction } => {
                    removal.require_transaction(&caller, &transaction)?;

                    let mut agents = self
                        .agents
                        .lock()
                        .map_err(|_| IpcError::new("internal", "Agent lock poisoned"))?;

                    agents.retain(|session, a| {
                        windows::process_identity(a.pid)
                            .is_ok_and(|p| p.sid == self.config.owner_sid && p.session == *session)
                    });

                    if (agents.is_empty()
                        && (self.browser_cleanup_required()?
                            || removal
                                .transaction
                                .as_ref()
                                .is_some_and(|(_, _, erase)| *erase)))
                        || !agents.iter().all(|(session, a)| {
                            removal.acknowledgements.contains(&(*session, a.pid))
                        })
                    {
                        return Ok(removal.reply(transaction, "draining"));
                    }

                    if self.config.enforce {
                        crate::hosts::apply(&[]).map_err(|e| IpcError::new("cleanup_failed", e))?;
                    }

                    self.journal_removal(&format!("ready:{transaction}"))?;

                    removal.ready = true;

                    Ok(removal.reply(transaction, "ready"))
                }

                MaintenanceRequest::AbortRemoval { transaction } => {
                    removal.require_transaction(&caller, &transaction)?;

                    if removal.ready {
                        return Err(IpcError::new(
                            "maintenance_committed",
                            "Use explicit administrator recovery after cleanup begins",
                        ));
                    }

                    self.journal_removal("")?;

                    removal.transaction = None;

                    removal.acknowledgements.clear();

                    self.revision_changed(removal)?;

                    Ok(removal.reply(transaction, "aborted"))
                }
                MaintenanceRequest::PrepareCompleteRemoval
                | MaintenanceRequest::ConsumeCompleteRemoval { .. } => unreachable!(),
            }
        }
    }

    fn native(
        &self,

        request: focuser_common::ipc::IpcRequest,
    ) -> Result<focuser_common::ipc::IpcResponse, IpcError> {
        use focuser_common::ipc::{IpcRequest as R, IpcResponse as P};

        match request {
            R::Ping => Ok(P::Pong),

            R::ExtensionEvent(event) => {
                use focuser_common::extension::ExtensionEvent as E;

                match event {
                    E::Connected { browser, .. } => crate::browser_api::record_extension_heartbeat(
                        &format!("{browser:?}"),
                        None,
                    ),

                    E::Disconnected { browser } => crate::browser_api::forget_browser(&browser),

                    E::Blocked { url, .. } => {
                        let _ = crate::browser_api::dispatch(
                            &BrowserRequest {
                                method: "POST".into(),

                                path: "/api/blocked".into(),

                                body: serde_json::json!({"url":url}).to_string(),

                                browser: None,
                            },
                            &self.context,
                        );
                    }

                    E::UsageReport {
                        domain, seconds, ..
                    } => {
                        let _ = crate::browser_api::dispatch(&BrowserRequest { method: "POST".into(), path: "/api/allowance-tick".into(), body: serde_json::json!({"hostname":domain,"active":true,"source":"native","increment_secs":seconds.min(5)}).to_string(), browser: None }, &self.context);
                    }

                    E::RequestRules => {}
                }

                Ok(P::Ok)
            }

            R::GetExtensionRules => {
                let engine = self
                    .context
                    .engine
                    .lock()
                    .map_err(|_| IpcError::new("internal", "Engine lock poisoned"))?;

                let rules = engine.compile_extension_rules_with_allowances(
                    &self.context.allowance_exempt_domains(&engine),
                    &self.context.allowance_tracker.blocked_domains(),
                );

                Ok(P::ExtensionRules(rules))
            }

            R::CheckDomain(domain) => {
                let result = self.command_inner(Command::CheckDomain { domain })?;

                match result {
                    CommandResult::Flag(value) => Ok(P::DomainBlocked(value)),

                    _ => Err(IpcError::new("protocol", "Unexpected domain result")),
                }
            }

            // This old bridge must not become a second administrative API.
            _ => Err(IpcError::new(
                "unsupported",
                "Native messaging accepts only ping, domain checks and compiled rules; browser mutations use the restricted browser protocol",
            )),
        }
    }
}

fn same_image(actual: &Path, expected: &Path) -> bool {
    match (actual.canonicalize(), expected.canonicalize()) {
        (Ok(a), Ok(b)) => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),

        _ => false,
    }
}

fn policy_mutation(command: &Command) -> bool {
    !matches!(
        command,
        Command::ListBlockLists
            | Command::GetStats { .. }
            | Command::GetBlockedEvents { .. }
            | Command::GetStatsRetention
            | Command::GetScheduledProtectionStatus
            | Command::GetSharedAllowanceStatus
            | Command::GetProtectionStatus
            | Command::GetSetting { .. }
            | Command::GetBlockingHealth
            | Command::PomodoroStatus
            | Command::PomodoroPresets
            | Command::PomodoroDrainEvents
            | Command::PomodoroHistory { .. }
            | Command::AllowanceList
            | Command::AllowanceDrainNotifications
            | Command::AllowanceHistory { .. }
            | Command::ExportConfiguration
            | Command::CheckDomain { .. }
            | Command::GetBrowserStatus
            | Command::GetAppIcons { .. }
            | Command::AppVersion
    )
}

#[cfg(test)]
mod kernel_guard_integration_tests {
    use super::*;
    struct Observer {
        database: PathBuf,
        events: Arc<Mutex<Vec<(u32, i64, u64)>>>,
        fail_commit: Arc<std::sync::atomic::AtomicBool>,
    }
    impl crate::kernel_guard::Transport for Observer {
        fn exchange(&mut self, message: [u8; 80]) -> Result<[u8; 56], IpcError> {
            let op = u32::from_le_bytes(message[8..12].try_into().unwrap());
            let db = rusqlite::Connection::open_with_flags(
                &self.database,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            let count = db
                .query_row("SELECT count(*) FROM block_lists", [], |r| r.get(0))
                .unwrap_or(-1);
            let until = u64::from_le_bytes(message[32..40].try_into().unwrap());
            self.events.lock().unwrap().push((op, count, until));
            if op == 2 && self.fail_commit.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(IpcError::new(
                    "kernel_guard_timeout",
                    "Test interrupted synchronization",
                ));
            }
            let mut reply = [0; 56];
            reply[..8].copy_from_slice(&message[..8]);
            reply[6..8].copy_from_slice(&56u16.to_le_bytes());
            reply[16..56].copy_from_slice(&message[16..56]);
            Ok(reply)
        }
    }
    #[test]
    fn stored_mutation_is_fenced_before_writing_and_disconnect_blocks_the_next_write() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("authority.db");
        let authority = Authority::open(
            &database,
            Configuration {
                owner_sid: "S-1-5-21-test".into(),
                agent_image: "agent.exe".into(),
                native_image: "native.exe".into(),
                enforce: false,
            },
        )
        .unwrap();
        let id = authority.removal.lock().unwrap().installation.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
        authority
            .attach_kernel_guard(crate::kernel_guard::Controller::new(
                Box::new(Observer {
                    database: database.clone(),
                    events: events.clone(),
                    fail_commit: fail.clone(),
                }),
                *uuid::Uuid::parse_str(&id).unwrap().as_bytes(),
            ))
            .unwrap();
        events.lock().unwrap().clear();
        authority
            .command(Command::CreateBlockList {
                name: "First".into(),
            })
            .unwrap();
        let observed = events.lock().unwrap().clone();
        assert_eq!(observed.first(), Some(&(1, 0, 0)));
        assert_eq!(observed.last(), Some(&(2, 1, 0)));
        fail.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            authority
                .command(Command::CreateBlockList {
                    name: "Persisted before disconnect".into()
                })
                .unwrap_err()
                .code,
            "kernel_guard_timeout"
        );
        assert_eq!(
            authority
                .command(Command::CreateBlockList {
                    name: "Must not be written".into()
                })
                .unwrap_err()
                .code,
            "kernel_guard_disconnected"
        );
        let db = rusqlite::Connection::open_with_flags(
            database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT count(*) FROM block_lists", [], |r| r.get(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn a_stale_weak_cache_and_storage_failure_cannot_clear_the_stored_commitment() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("authority.db");
        let authority = Authority::open(
            &database,
            Configuration {
                owner_sid: "S-1-5-21-test".into(),
                agent_image: "agent.exe".into(),
                native_image: "native.exe".into(),
                enforce: false,
            },
        )
        .unwrap();
        let CommandResult::BlockList(mut list) = authority
            .command(Command::CreateBlockList {
                name: "Stored protection".into(),
            })
            .unwrap()
        else {
            panic!("List required")
        };
        list.protection = Some(focuser_common::types::Protection::for_duration(600));
        authority
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .update_block_list(&list)
            .unwrap();
        assert!(
            authority.context.engine.lock().unwrap().block_lists()[0]
                .protection
                .is_none()
        );
        let id = authority.removal.lock().unwrap().installation.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        authority
            .attach_kernel_guard(crate::kernel_guard::Controller::new(
                Box::new(Observer {
                    database: database.clone(),
                    events: events.clone(),
                    fail_commit: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                }),
                *uuid::Uuid::parse_str(&id).unwrap().as_bytes(),
            ))
            .unwrap();
        assert!(events.lock().unwrap().last().unwrap().2 > 0);
        // Break only this disposable DB. Failed reads may not manufacture an
        // empty snapshot and release the kernel's existing commitment/fence.
        rusqlite::Connection::open(&database)
            .unwrap()
            .execute("DROP TABLE block_lists", [])
            .unwrap();
        events.lock().unwrap().clear();
        assert!(
            authority
                .command(Command::CreateBlockList {
                    name: "Must fail".into()
                })
                .is_err()
        );
        assert!(events.lock().unwrap().iter().any(|event| event.0 == 1));
        assert!(!events.lock().unwrap().iter().any(|event| event.0 == 2));
    }
}

#[cfg(test)]
mod update_window_tests {
    use super::*;
    use chrono::Datelike;
    use focuser_common::types::{BlockList, Protection, Schedule, ScheduledProtection, TimeSlot};
    #[test]
    fn real_activation_decision_covers_schedule_gaps_disabled_breaks_and_strict_budgets() {
        let dir = tempfile::tempdir().unwrap();
        let a = Authority::open(
            &dir.path().join("policy.db"),
            Configuration {
                owner_sid: "test".into(),
                agent_image: std::env::current_exe().unwrap(),
                native_image: std::env::current_exe().unwrap(),
                enforce: false,
            },
        )
        .unwrap();
        let mut list = BlockList::new("manual commitment during schedule gap");
        let now = chrono::Utc::now();
        list.protection = Some(Protection {
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            started_at: now,
            expires_at: Some(now + chrono::Duration::hours(1)),
        });
        list.schedule = Some(Schedule {
            id: uuid::Uuid::new_v4(),
            name: "tomorrow".into(),
            enabled: true,
            time_slots: vec![TimeSlot {
                day: chrono::Local::now().weekday().succ(),
                start: chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                end: chrono::NaiveTime::from_hms_opt(1, 0, 0).unwrap(),
            }],
        });
        assert!(!list.is_effectively_active());
        {
            let mut e = a.context.engine.lock().unwrap();
            e.db().create_block_list(&list).unwrap();
            e.refresh().unwrap();
        }
        let before = serde_json::to_string(&list).unwrap();
        assert!(a.activation_blocked().unwrap());
        assert_eq!(
            serde_json::to_string(&a.context.engine.lock().unwrap().block_lists()[0]).unwrap(),
            before
        );
        list.enabled = false;
        {
            let mut e = a.context.engine.lock().unwrap();
            e.db().update_block_list(&list).unwrap();
            e.refresh().unwrap();
        }
        assert!(!a.activation_blocked().unwrap());
        list.enabled = true;
        list.protection = None;
        list.scheduled_protection = Some(ScheduledProtection { lock: None });
        {
            let mut e = a.context.engine.lock().unwrap();
            e.db().update_block_list(&list).unwrap();
            e.refresh().unwrap();
        }
        assert!(a.activation_blocked().unwrap());
        list.enabled = false;
        {
            let mut e = a.context.engine.lock().unwrap();
            e.db().update_block_list(&list).unwrap();
            e.refresh().unwrap();
            let allowance = focuser_common::allowance::Allowance::new(
                focuser_common::allowance::AllowanceMatch::Domain("example.test".into()),
                60,
                true,
            );
            e.db().create_allowance(&allowance).unwrap();
        }
        assert!(a.activation_blocked().unwrap());
    }
}

#[cfg(all(test, windows))]
mod broker_loss_tests {
    use super::*;
    use crate::update::{Envelope, Journal, Manager, Phase, Store};
    use base64::Engine as _;
    #[test]
    fn confirmed_broker_loss_in_original_live_generation_does_not_release_commitments() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = Authority::open(
            &dir.path().join("policy.db"),
            Configuration {
                owner_sid: "test".into(),
                agent_image: std::env::current_exe().unwrap(),
                native_image: std::env::current_exe().unwrap(),
                enforce: false,
            },
        )
        .unwrap();
        let CommandResult::BlockList(list) = a
            .command(Command::CreateBlockList {
                name: "keep commitment".into(),
            })
            .unwrap()
        else {
            panic!()
        };
        a.command(Command::EnableProtection {
            list_id: list.id,
            duration_minutes: Some(60),
            prevent_uninstall: true,
            prevent_service_stop: true,
            prevent_modification: true,
            lock: None,
        })
        .unwrap();
        let installation = a
            .context
            .engine
            .lock()
            .unwrap()
            .db()
            .get_setting("_authority.installation")
            .unwrap()
            .unwrap();
        let envelope: Envelope =
            serde_json::from_str(include_str!("../tests/fixtures/update/valid.json")).unwrap();
        let body = base64::engine::general_purpose::STANDARD
            .decode(&envelope.manifest)
            .unwrap();
        let mut j = Journal::fresh(&installation, 0, 1);
        j.manifest = Some(serde_json::from_slice(&body).unwrap());
        j.envelope = Some(envelope);
        j.artifact_digest = crate::update::digest(&body);
        j.phase = Phase::Authorized;
        j.origin_generation = Some(a.generation.clone());
        j.broker_pid = Some(u32::MAX);
        j.checkpoint = Some(crate::update_broker::policy_digest(&a.database).unwrap());
        let store = Store::new(dir.path().join("updates"), false).unwrap();
        store.save(&j).unwrap();
        a.updates = Some(Arc::new(Mutex::new(Manager::open(store.clone()).unwrap())));
        a.tick().unwrap();
        let recovered = store.load().unwrap().unwrap();
        assert_eq!(recovered.phase, Phase::RolledBack);
        assert_eq!(recovered.code, "broker_lost_before_service_stop");
        assert_eq!(
            a.command(Command::DeleteBlockList { id: list.id })
                .unwrap_err()
                .code,
            "protected"
        );
        assert!(!a.can_stop());
        let mut validating = recovered;
        validating.phase = Phase::Validating;
        store.save(&validating).unwrap();
        a.updates.as_ref().unwrap().lock().unwrap().journal = Some(validating.clone());
        validating.phase = Phase::Complete;
        store.save(&validating).unwrap();
        assert!(
            !a.update_paused(),
            "a fresh agent must observe broker completion before the next tick"
        );
        assert!(!a.update_frozen());
    }
}

#[cfg(test)]
mod removal_client_exit_tests {
    use super::*;
    #[test]
    fn complete_removal_denies_pomodoro_breaks_and_pauses_after_authority_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.db");
        let config = Configuration {
            owner_sid: "owner".into(),
            agent_image: std::env::current_exe().unwrap(),
            native_image: std::env::current_exe().unwrap(),
            enforce: false,
        };
        let a = Authority::open(&path, config.clone()).unwrap();
        let mut list = focuser_common::types::BlockList::new("Pomodoro");
        list.enabled = false;
        list.websites
            .push(focuser_common::types::WebsiteRule::domain("example.com"));
        a.context
            .engine
            .lock()
            .unwrap()
            .db()
            .create_block_list(&list)
            .unwrap();
        a.command(Command::PomodoroStart {
            block_list_id: list.id,
            config: focuser_common::pomodoro::PomodoroConfig::CLASSIC,
        })
        .unwrap();
        a.command(Command::PomodoroSkip).unwrap(); // Short break disables its list.
        assert!(!a.removal_decision_for(true).unwrap().allowed);
        a.command(Command::PomodoroPause).unwrap();
        drop(a); // No UI lifetime or volatile unlocked flag owns this decision.
        let a = Authority::open(&path, config).unwrap();
        let denied = a.removal_decision_for(true).unwrap();
        assert!(!denied.allowed);
        assert_eq!(denied.blockers[0].reason, "pomodoro_session");
        assert!(
            crate::removal::explanation(&denied).contains("Pomodoro session is completed or ended")
        );
        a.command(Command::PomodoroStop).unwrap();
        assert!(a.removal_decision_for(true).unwrap().allowed);
    }
    #[test]
    fn complete_cleanup_uses_stored_blocks_and_rejects_legacy_agent_acknowledgement() {
        let dir = tempfile::tempdir().unwrap();
        let image = std::env::current_exe().unwrap();
        let a = Authority::open(
            &dir.path().join("policy.db"),
            Configuration {
                owner_sid: "S-1-5-21-test".into(),
                agent_image: image.clone(),
                native_image: image.clone(),
                enforce: false,
            },
        )
        .unwrap();
        let mut list = focuser_common::types::BlockList::new("Running");
        list.websites
            .push(focuser_common::types::WebsiteRule::domain("example.com"));
        a.context
            .engine
            .lock()
            .unwrap()
            .db()
            .create_block_list(&list)
            .unwrap();
        assert!(a.removal_decision().unwrap().allowed);
        assert!(!a.removal_decision_for(true).unwrap().allowed);
        list.enabled = false;
        a.context
            .engine
            .lock()
            .unwrap()
            .db()
            .update_block_list(&list)
            .unwrap();
        let peer = Peer {
            sid: a.config.owner_sid.clone(),
            pid: std::process::id(),
            session: 1,
            image,
        };
        let token = {
            let mut removal = a.removal.lock().unwrap();
            let caller = crate::removal::Caller::from(&peer);
            let token = removal
                .issue(caller.clone(), true, Instant::now())
                .unwrap()
                .transaction;
            removal
                .consume(&caller, &token, true, Instant::now())
                .unwrap();
            token
        };
        let call = |body| {
            a.handle(
                &peer,
                Request {
                    version: VERSION,
                    body,
                },
            )
        };
        assert_eq!(
            call(RequestBody::Agent(AgentRequest::MaintenanceReady {
                transaction: token.clone()
            }))
            .unwrap_err()
            .code,
            "cleanup_scope_mismatch"
        );
        assert!(a.removal.lock().unwrap().acknowledgements.is_empty());
        let ReplyBody::Agent(snapshot) =
            call(RequestBody::Agent(AgentRequest::Poll { after_event: 0 })).unwrap()
        else {
            panic!()
        };
        assert!(snapshot.delete_user_data);
        assert!(!snapshot.shutdown);
        let ReplyBody::Removal(decision) = call(RequestBody::RemovalDecision).unwrap() else {
            panic!()
        };
        assert!(
            decision.shutdown_clients,
            "Only UI/native clients exit after complete-removal consumption; the agent must finish cleanup first"
        );
        call(RequestBody::Agent(AgentRequest::CompleteCleanupReady {
            transaction: token,
        }))
        .unwrap();
        assert_eq!(a.removal.lock().unwrap().acknowledgements.len(), 1);
    }
    #[test]
    fn idle_clean_removal_does_not_require_an_agent_but_unknown_or_pending_cleanup_does() {
        let dir = tempfile::tempdir().unwrap();
        let config = Configuration {
            owner_sid: "owner".into(),
            agent_image: std::env::current_exe().unwrap(),
            native_image: std::env::current_exe().unwrap(),
            enforce: false,
        };
        let db = dir.path().join("policy.db");
        let a = Authority::open(&db, config.clone()).unwrap();
        assert!(!a.agent_connected());
        assert_eq!(
            a.ensure_removal_coverage().unwrap_err().code,
            "agent_missing"
        );
        let peer = Peer {
            sid: "owner".into(),
            pid: std::process::id(),
            session: 1,
            image: std::env::current_exe().unwrap(),
        };
        a.handle(
            &peer,
            Request {
                version: VERSION,
                body: RequestBody::Agent(AgentRequest::BrowserRestored),
            },
        )
        .unwrap();
        assert!(a.ensure_removal_coverage().is_ok());
        drop(a);
        let a = Authority::open(&db, config).unwrap();
        assert!(a.ensure_removal_coverage().is_ok());
        {
            let e = a.context.engine.lock().unwrap();
            e.db()
                .set_setting("_authority.browser_cleanup_required", "true")
                .unwrap();
        }
        assert_eq!(
            a.ensure_removal_coverage().unwrap_err().code,
            "agent_missing"
        );
    }
    #[test]
    fn cooperative_exit_requires_consumed_and_ready_removal_not_a_ui_flag_or_allowed_decision() {
        let dir = tempfile::tempdir().unwrap();
        let a = Authority::open(
            &dir.path().join("policy.db"),
            Configuration {
                owner_sid: "owner".into(),
                agent_image: std::env::current_exe().unwrap(),
                native_image: std::env::current_exe().unwrap(),
                enforce: false,
            },
        )
        .unwrap();
        let peer = Peer {
            sid: "owner".into(),
            pid: std::process::id(),
            session: 1,
            image: std::env::current_exe().unwrap(),
        };
        let decision = || match a
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
            _ => panic!(),
        };
        assert!(!decision().shutdown_clients);
        let token = {
            let mut removal = a.removal.lock().unwrap();
            removal
                .issue(crate::removal::Caller::from(&peer), false, Instant::now())
                .unwrap()
                .transaction
        };
        assert!(!decision().shutdown_clients);
        {
            let mut removal = a.removal.lock().unwrap();
            removal
                .consume(
                    &crate::removal::Caller::from(&peer),
                    &token,
                    false,
                    Instant::now(),
                )
                .unwrap();
        }
        assert!(!decision().shutdown_clients);
        a.removal.lock().unwrap().ready = true;
        assert_eq!(
            a.handle(
                &peer,
                Request {
                    version: VERSION,
                    body: RequestBody::Agent(AgentRequest::BrowserRecoveryIntent)
                }
            )
            .unwrap_err()
            .code,
            "maintenance_in_progress",
            "a stale active snapshot cannot reapply restrictions during removal"
        );
        assert!(decision().shutdown_clients);
        // Even a corrupt ready marker cannot close a protected client.
        {
            let mut e = a.context.engine.lock().unwrap();
            let mut list = focuser_common::types::BlockList::new("Still protected");
            list.protection = Some(focuser_common::types::Protection::for_duration(60));
            e.db().create_block_list(&list).unwrap();
            e.refresh().unwrap();
        }
        assert!(!decision().shutdown_clients);
    }
}
