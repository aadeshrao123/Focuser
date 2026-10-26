//! Elevated, explicit provisioning/removal helper. No command here is run by a
//! development authority. The installed service executable is also the helper;
//! there is no user-writable maintenance executable or updater trust flag.
use crate::storage::*;
use focuser_ipc::*;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_service::{service::*, service_manager::*};
use winreg::{RegKey, enums::*};
pub const AGENT_TASK: &str = r"\Focuser\SessionAgent";
const NATIVE_KEYS: [&str; 4] = [
    r"Software\Google\Chrome\NativeMessagingHosts\com.focuser.native",
    r"Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native",
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.focuser.native",
    r"Software\Mozilla\NativeMessagingHosts\com.focuser.native",
];
fn fail(code: &str, e: impl std::fmt::Display) -> IpcError {
    IpcError::new(code, e.to_string())
}
pub(crate) fn elevated() -> Result<PathBuf, IpcError> {
    windows::require_elevated_process(std::process::id())?;
    let exe = std::env::current_exe().map_err(|e| fail("configuration", e))?;
    verify_code(&exe)?;
    Ok(exe)
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn agent_task(owner: &str, image: &Path, installation: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
<RegistrationInfo><Description>Focuser session agent {installation}</Description><URI>\Focuser\SessionAgent</URI></RegistrationInfo>
<Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{owner}</UserId></LogonTrigger></Triggers>
<Principals><Principal id="Owner"><UserId>{owner}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings>
<Actions Context="Owner"><Exec><Command>{image}</Command></Exec></Actions></Task>"#,
        owner = xml(owner),
        image = xml(&image.to_string_lossy()),
        installation = xml(installation)
    )
}
fn task_child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|n| {
        n.is_element()
            && n.tag_name().name() == name
            && n.tag_name().namespace()
                == Some("http://schemas.microsoft.com/windows/2004/02/mit/task")
    })
}
pub fn task_belongs_to_installation(
    task: &str,
    owner: &str,
    image: &Path,
    installation: &str,
) -> bool {
    if task.len() > 256 * 1024 {
        return false;
    }
    let Ok(document) = roxmltree::Document::parse(task) else {
        return false;
    };
    let root = document.root_element();
    let Some(registration) = task_child(root, "RegistrationInfo") else {
        return false;
    };
    if task_child(registration, "Description")
        .and_then(|n| n.text())
        .map(str::trim)
        != Some(format!("Focuser session agent {installation}").as_str())
    {
        return false;
    }
    let Some(principals) = task_child(root, "Principals") else {
        return false;
    };
    let principals: Vec<_> = principals.children().filter(|n| n.is_element()).collect();
    if principals.len() != 1 {
        return false;
    }
    let principal = principals[0];
    if task_child(principal, "UserId")
        .and_then(|n| n.text())
        .map(str::trim)
        != Some(owner)
        // Task Scheduler omits the default LeastPrivilege value when exporting
        // a registered task. An explicit elevated or empty value is still invalid.
        || task_child(principal, "RunLevel")
            .is_some_and(|n| n.text().map(str::trim) != Some("LeastPrivilege"))
        || task_child(principal, "LogonType")
            .and_then(|n| n.text())
            .map(str::trim)
            != Some("InteractiveToken")
    {
        return false;
    }
    let Some(actions) = task_child(root, "Actions") else {
        return false;
    };
    if actions.attribute("Context").unwrap_or("") != principal.attribute("id").unwrap_or("") {
        return false;
    }
    let executors: Vec<_> = actions.children().filter(|n| n.is_element()).collect();
    if executors.len() != 1 || executors[0].tag_name().name() != "Exec" {
        return false;
    }
    let executor = executors[0];
    let Some(command) = task_child(executor, "Command").and_then(|n| n.text()) else {
        return false;
    };
    Path::new(command.trim()).eq_ignore_ascii_case_path(image)
        && task_child(executor, "Arguments")
            .and_then(|n| n.text())
            .is_none_or(|args| args.trim().is_empty())
}

pub(crate) fn tool(name: &str, args: &[&str]) -> Result<(i32, String), IpcError> {
    use std::os::windows::process::CommandExt;
    let mut dir = vec![0u16; 32768];
    let n =
        unsafe { ::windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut dir)) }
            as usize;
    if n == 0 || n >= dir.len() {
        return Err(fail("configuration", "Cannot locate System32"));
    }
    let exe = PathBuf::from(String::from_utf16_lossy(&dir[..n])).join(name);
    let output_path = production_root()?.join(format!("operation-{}.log", uuid::Uuid::new_v4()));
    secure_file(&output_path)?;
    let output = std::fs::OpenOptions::new()
        .write(true)
        .open(&output_path)
        .map_err(|e| fail("provisioning_failed", e))?;
    let mut child = std::process::Command::new(exe)
        .args(args)
        .creation_flags(0x08000000)
        .stdout(
            output
                .try_clone()
                .map_err(|e| fail("provisioning_failed", e))?,
        )
        .stderr(output)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| fail("provisioning_failed", e))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let code = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| fail("provisioning_failed", e))?
        {
            break status.code().unwrap_or(1);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(fail(
                "maintenance_timeout",
                "Windows provisioning command timed out; inspect operation logs",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let bytes = std::fs::read(&output_path).map_err(|e| fail("provisioning_failed", e))?;
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.get(1) == Some(&0) {
        let bytes = if bytes.starts_with(&[0xff, 0xfe]) {
            &bytes[2..]
        } else {
            &bytes[..]
        };
        String::from_utf16_lossy(
            &bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    if code == 0 {
        let _ = std::fs::remove_file(output_path);
    }
    Ok((code, text))
}
pub(crate) fn checked_tool(name: &str, args: &[&str]) -> Result<(), IpcError> {
    let (code, text) = tool(name, args)?;
    if code != 0 {
        return Err(fail(
            "cleanup_failed",
            format!("{name} returned {code}: {text}"),
        ));
    }
    Ok(())
}
fn installation_id(root: &Path) -> Result<String, IpcError> {
    let connection = rusqlite::Connection::open_with_flags(
        root.join("authority.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| fail("storage", e))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| fail("storage", e))?;
    use rusqlite::OptionalExtension;
    connection
        .query_row(
            "SELECT value FROM settings WHERE key='_authority.installation'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| fail("storage", e))?
        .ok_or_else(|| {
            fail(
                "not_initialized",
                "Start the provisioned authority before registering the agent",
            )
        })
}
fn manager(access: ServiceManagerAccess) -> Result<ServiceManager, IpcError> {
    ServiceManager::local_computer(None::<&str>, access).map_err(|e| fail("provisioning_failed", e))
}
fn info(exe: &Path) -> ServiceInfo {
    ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: "Focuser policy authority".into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.into(),
        launch_arguments: vec!["service".into()],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    }
}
fn verify_service(service: &Service, exe: &Path) -> Result<(), IpcError> {
    let config = service
        .query_config()
        .map_err(|e| fail("provisioning_failed", e))?;
    let command = config.executable_path.to_string_lossy();
    // windows-service exposes the complete SCM command in executable_path.
    let expected = format!("\"{}\" service", exe.display());
    let unquoted = format!("{} service", exe.display());
    if !(command.eq_ignore_ascii_case(&expected)
        || (!exe.to_string_lossy().contains(char::is_whitespace)
            && command.eq_ignore_ascii_case(&unquoted)))
        || config
            .account_name
            .as_ref()
            .is_none_or(|n| !n.to_string_lossy().eq_ignore_ascii_case("LocalSystem"))
    {
        return Err(fail(
            "installation_mismatch",
            "Existing SCM entry is not this Focuser installation",
        ));
    }
    Ok(())
}
trait PathEqual {
    fn eq_ignore_ascii_case_path(&self, other: &Path) -> bool;
}
impl PathEqual for Path {
    fn eq_ignore_ascii_case_path(&self, other: &Path) -> bool {
        self.to_string_lossy()
            .eq_ignore_ascii_case(&other.to_string_lossy())
    }
}
/// Fresh provision OR repair the same protected binaries/configuration. Never
/// overwrite existing code or an authoritative database. Updates are separate.
pub fn install(
    owner: String,
    source: Option<PathBuf>,
    empty: bool,
    repair: bool,
) -> Result<(), IpcError> {
    let exe = elevated()?;
    install_at(&exe, owner, source, empty, repair)
}

/// Same-code setup repair can run from the installer's private bundled helper.
/// The caller verifies the installed helper hash before entering this path.
pub(crate) fn install_at(
    exe: &Path,
    owner: String,
    source: Option<PathBuf>,
    empty: bool,
    repair: bool,
) -> Result<(), IpcError> {
    if cfg!(feature = "kernel-file-guard") {
        return Err(fail(
            "kernel_guard_provisioning_deferred",
            "Experimental kernel builds cannot provision or repair a live installation. Signed-driver enrollment, driver-aware removal and OS acceptance are not certified.",
        ));
    }
    verify_code(exe)?;
    let dir = exe
        .parent()
        .ok_or_else(|| fail("configuration", "No installation directory"))?;
    windows::validate_sid(&owner)?;
    if owner == windows::SYSTEM_SID {
        return Err(fail("configuration", "Enroll one interactive user SID"));
    }
    for binary in [
        "focuser-ui.exe",
        "focuser-cli.exe",
        "focuser-native.exe",
        "focuser-session-agent.exe",
    ] {
        verify_code(&dir.join(binary))?;
    }
    #[cfg(target_env = "gnu")]
    verify_code(&dir.join("WebView2Loader.dll"))?;
    let config = ServiceConfiguration {
        version: VERSION,
        owner_sid: owner.clone(),
        agent_image: dir.join("focuser-session-agent.exe"),
        native_image: dir.join("focuser-native.exe"),
    };
    let root = production_root()?;
    let existed = root.exists();
    if repair && !root.exists() {
        return Err(fail(
            "not_initialized",
            "Repair requires existing authoritative storage",
        ));
    }
    if !root.exists() && (source.is_some() == empty) {
        return Err(fail(
            "migration_required",
            "Explicitly select --source or --initialize-empty",
        ));
    }
    if let Some(source) = &source {
        validate_source(source)?;
    }
    provision(&config, false)?;
    let _maintenance_lock = StorageLock::maintenance(&root)?;
    crate::update_broker::ensure_idle(&root)?;
    validate_initialization(
        existed,
        root.join("authority.db").exists(),
        root.join("removal-progress.json").exists(),
        empty,
        repair,
    )?;
    if !root.join("authority.db").exists() {
        let _lock = StorageLock::acquire(&root)?;
        if let Some(source) = source {
            migrate(&source, &root.join("authority.db"), true)?;
        } else if empty {
            secure_file(&root.join("authority.db"))?;
            let db = focuser_core::Database::open(root.join("authority.db"))
                .map_err(|e| fail("storage", e))?;
            // A newly initialized, empty installation has never applied browser
            // policies. Failed initial agent startup must not strand its removal.
            db.set_setting("_authority.browser_cleanup_required", "false")
                .map_err(|e| fail("storage", e))?;
        } else {
            return Err(fail(
                "migration_required",
                "Existing incomplete provisioning needs an explicit data source",
            ));
        }
    } else if source.is_some() {
        return Err(fail(
            "already_initialized",
            "Existing authority is preserved; import is not an upgrade",
        ));
    }
    verify_private(&root.join("authority.db"))?;
    {
        let connection = rusqlite::Connection::open_with_flags(
            root.join("authority.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|e| fail("storage", e))?;
        use rusqlite::OptionalExtension;
        let journal: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key='_authority.maintenance'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| fail("storage", e))?;
        if journal.is_some_and(|j| !j.is_empty()) {
            return Err(fail(
                "maintenance_interrupted",
                "Repair preserves policy but cannot clear a removal journal; use explicit administrator recovery",
            ));
        }
    }
    let scm = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let access = ServiceAccess::QUERY_CONFIG
        | ServiceAccess::CHANGE_CONFIG
        | ServiceAccess::QUERY_STATUS
        | ServiceAccess::START;
    let service = match scm.open_service(SERVICE_NAME, access) {
        Ok(service) => {
            verify_service(&service, exe)?;
            service
                .change_config(&info(exe))
                .map_err(|e| fail("provisioning_failed", e))?;
            service
        }
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1060) => scm
            .create_service(&info(exe), access)
            .map_err(|e| fail("provisioning_failed", e))?,
        Err(e) => return Err(fail("provisioning_failed", e)),
    };
    checked_tool(
        "sc.exe",
        &[
            "sdset",
            SERVICE_NAME,
            "D:P(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;BU)",
        ],
    )?;
    service
        .update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86400)),
            reboot_msg: Some(OsString::new()),
            command: Some(OsString::new()),
            actions: Some(vec![
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(5),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(10),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(30),
                },
                ServiceAction {
                    action_type: ServiceActionType::None,
                    delay: Duration::ZERO,
                },
            ]),
        })
        .map_err(|e| fail("provisioning_failed", e))?;
    service
        .set_failure_actions_on_non_crash_failures(true)
        .map_err(|e| fail("provisioning_failed", e))?;
    if service
        .query_status()
        .map_err(|e| fail("provisioning_failed", e))?
        .current_state
        != ServiceState::Running
    {
        service
            .start::<&str>(&[])
            .map_err(|e| fail("provisioning_failed", e))?;
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let id = loop {
        match installation_id(&root) {
            Ok(id) => break id,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e),
        }
    };
    crate::update_broker::provision_recovery(&root, exe, &id)?;
    let task = agent_task(&owner, &config.agent_image, &id);
    let (code, current) = tool("schtasks.exe", &["/Query", "/TN", AGENT_TASK, "/XML"])?;
    if code == 0 && !task_belongs_to_installation(&current, &owner, &config.agent_image, &id) {
        return Err(fail(
            "installation_mismatch",
            "Existing session task is not this installation",
        ));
    }
    let task_path = root.join("session-agent.xml");
    if !task_path.exists() {
        secure_file(&task_path)?;
    } else {
        verify_private(&task_path)?;
    }
    let bytes: Vec<u8> = std::iter::once(0xfeffu16)
        .chain(task.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(&task_path, bytes).map_err(|e| fail("provisioning_failed", e))?;
    checked_tool(
        "schtasks.exe",
        &[
            "/Create",
            "/TN",
            AGENT_TASK,
            "/XML",
            &task_path.to_string_lossy(),
            "/F",
        ],
    )?;
    register_native(dir)?;
    // InteractiveToken never launches an elevated process or logs the user in.
    let (code, message) = tool("schtasks.exe", &["/Run", "/TN", AGENT_TASK])?;
    if code != 0 {
        tracing::warn!(code,%message,"agent_pending_logon: owner task is registered; immediate startup failed. Coverage remains degraded until the enrolled user logs on or task startup is repaired");
    }
    Ok(())
}
fn register_native(dir: &Path) -> Result<(), IpcError> {
    let chromium = serde_json::json!({"name":"com.focuser.native","description":"Focuser Native Messaging Host","path":dir.join("focuser-native.exe"),"type":"stdio","allowed_origins":["chrome-extension://jpnhbpbcmagoonmaleppldmcnaibkbmj/","chrome-extension://kdkmjbjegdcjdlbciifigfkifookbppg/"]});
    let firefox = serde_json::json!({"name":"com.focuser.native","description":"Focuser Native Messaging Host","path":dir.join("focuser-native.exe"),"type":"stdio","allowed_extensions":["focuser@focuser-app"]});
    for (name, value) in [
        ("native-chromium.json", chromium),
        ("native-firefox.json", firefox),
    ] {
        let path = dir.join(name);
        if path.exists() {
            verify_code(&path)?;
        }
        write_if_changed(
            &path,
            &serde_json::to_vec_pretty(&value).map_err(|e| fail("provisioning_failed", e))?,
        )
        .map_err(|e| fail("provisioning_failed", e))?;
    }
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for (index, name) in NATIVE_KEYS.iter().enumerate() {
        let (key, _) = hklm
            .create_subkey_with_flags(name, KEY_WRITE | KEY_WOW64_64KEY)
            .map_err(|e| fail("provisioning_failed", e))?;
        key.set_value(
            "",
            &dir.join(if index == 3 {
                "native-firefox.json"
            } else {
                "native-chromium.json"
            })
            .to_string_lossy()
            .as_ref(),
        )
        .map_err(|e| fail("provisioning_failed", e))?;
    }
    Ok(())
}
fn request(body: MaintenanceRequest) -> Result<MaintenanceReply, IpcError> {
    match Client::default().request_blocking(RequestBody::Maintenance(body))? {
        ReplyBody::Maintenance(r) => Ok(r),
        _ => Err(fail("protocol", "Unexpected maintenance response")),
    }
}
fn progress(
    root: &Path,
    transaction: &str,
    phase: &str,
    delete_data: bool,
) -> Result<(), IpcError> {
    let stage = root.join(format!("removal-{}.tmp", uuid::Uuid::new_v4()));
    secure_file(&stage)?;
    let data =
        serde_json::json!({"transaction":transaction,"phase":phase,"delete_data":delete_data});
    std::fs::write(
        &stage,
        serde_json::to_vec(&data).map_err(|e| fail("storage", e))?,
    )
    .map_err(|e| fail("storage", e))?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&stage)
        .and_then(|f| f.sync_all())
        .map_err(|e| fail("storage", e))?;
    // Windows rename will not replace. Keep each completed step as an immutable
    // journal record; removal-progress.json is the initial durable tombstone.
    let target = if phase == "authorized" {
        root.join("removal-progress.json")
    } else {
        root.join(format!("removal-{phase}.json"))
    };
    std::fs::rename(stage, target).map_err(|e| fail("storage", e))?;
    Ok(())
}
/// Both direct helper execution and every NSIS uninstall mode use this path.
/// No task/SCM/registry/hosts/data/file deletion precedes service consumption.
fn wait_for_clients_exit(config: &ServiceConfiguration) -> Result<(), IpcError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let present = focuser_common::process::list().into_iter().any(|p| {
            windows::process_identity(p.pid).is_ok_and(|p| {
                p.sid == config.owner_sid
                    && (p.image.eq_ignore_ascii_case_path(&config.agent_image)
                        || p.image.eq_ignore_ascii_case_path(&config.native_image)
                        || config.agent_image.parent().is_some_and(|dir| {
                            p.image
                                .eq_ignore_ascii_case_path(&dir.join("focuser-ui.exe"))
                        }))
            })
        });
        if !present {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(fail(
                "client_shutdown_failed",
                "An open Focuser interface, agent or native bridge did not exit after authorized removal. Close Focuser clients and retry; removal is incomplete.",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_process_exit(
    handle: ::windows::Win32::Foundation::HANDLE,
    millis: u32,
) -> Result<(), IpcError> {
    use ::windows::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject};
    if unsafe { WaitForSingleObject(handle, millis) } != WAIT_OBJECT_0 {
        return Err(fail(
            "maintenance_timeout",
            "The stopped authority process has not exited; its storage was not erased",
        ));
    }
    Ok(())
}

pub fn remove(delete_data: bool) -> Result<(), IpcError> {
    let exe = elevated()?;
    let dir = exe.parent().unwrap();
    let root = production_root()?;
    verify_private(&root)?;
    let _maintenance_lock = StorageLock::maintenance(&root)?;
    crate::update_broker::ensure_idle(&root)?;
    verify_private(&root.join("service.json"))?;
    let config: ServiceConfiguration = serde_json::from_slice(
        &std::fs::read(root.join("service.json")).map_err(|e| fail("storage", e))?,
    )
    .map_err(|e| fail("storage", e))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    crate::removal::perform_removal(
        delete_data,
        request,
        || {
            if Instant::now() >= deadline {
                return Err(fail(
                    "agent_cleanup_timeout",
                    "Enrolled agent did not acknowledge cleanup. Close the Focuser interface and retry from the enrolled user's desktop; complete removal requires the updated session agent.",
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
            Ok(())
        },
        |transaction| {
            progress(&root, transaction, "authorized", delete_data)?;
            // Stop new browser-native host launches while the authority can
            // still deliver the committed shutdown to existing clients. These
            // registrations are removed only after service authorization.
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            for (index, name) in NATIVE_KEYS.iter().enumerate() {
                match hklm.open_subkey_with_flags(name, KEY_READ | KEY_WOW64_64KEY) {
                    Ok(key) => {
                        let value: String =
                            key.get_value("").map_err(|e| fail("cleanup_failed", e))?;
                        let expected = dir.join(if index == 3 {
                            "native-firefox.json"
                        } else {
                            "native-chromium.json"
                        });
                        if !Path::new(&value).eq_ignore_ascii_case_path(&expected) {
                            return Err(fail(
                                "installation_mismatch",
                                "Native host registration belongs to another installation",
                            ));
                        }
                        drop(key);
                        hklm.delete_subkey_with_flags(name, KEY_WOW64_64KEY)
                            .map_err(|e| fail("cleanup_failed", e))?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(fail("cleanup_failed", e)),
                }
            }
            // Keep IPC alive long enough for the committed shutdown directive.
            // This handles both task-launched and manually started production agents.
            wait_for_clients_exit(&config)?;
            let scm = manager(ServiceManagerAccess::CONNECT)?;
            let service = scm
                .open_service(
                    SERVICE_NAME,
                    ServiceAccess::STOP
                        | ServiceAccess::QUERY_STATUS
                        | ServiceAccess::QUERY_CONFIG
                        | ServiceAccess::DELETE,
                )
                .map_err(|e| fail("cleanup_failed", e))?;
            verify_service(&service, &exe)?;
            // SCM can report Stopped before the process releases its database
            // and file handles. Pin the actual service process before Stop, then
            // wait for its exit rather than racing private-storage deletion.
            let pid = service
                .query_status()
                .map_err(|e| fail("cleanup_failed", e))?
                .process_id
                .filter(|pid| *pid != 0)
                .ok_or_else(|| {
                    fail(
                        "cleanup_failed",
                        "The authority process could not be identified",
                    )
                })?;
            let identity = windows::process_identity(pid)?;
            if identity.sid != windows::SYSTEM_SID
                || !identity.image.eq_ignore_ascii_case_path(&exe)
            {
                return Err(fail(
                    "installation_mismatch",
                    "SCM process is not this authority",
                ));
            }
            let process = windows::OwnedHandle(
                unsafe {
                    ::windows::Win32::System::Threading::OpenProcess(
                        ::windows::Win32::System::Threading::PROCESS_SYNCHRONIZE,
                        false,
                        pid,
                    )
                }
                .map_err(|e| fail("cleanup_failed", e))?,
            );
            service.stop().map_err(|e| fail("cleanup_failed", e))?;
            let deadline = Instant::now() + Duration::from_secs(20);
            while service
                .query_status()
                .map_err(|e| fail("cleanup_failed", e))?
                .current_state
                != ServiceState::Stopped
            {
                if Instant::now() >= deadline {
                    return Err(fail(
                        "maintenance_timeout",
                        "Authority did not stop; no service deletion performed",
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            wait_for_process_exit(process.0, 20_000)?;
            progress(&root, transaction, "service-stopped", delete_data)?;
            // Task is Focuser's fixed, verified installation resource. End only its
            // process, never arbitrary shells/browsers/other scheduled tasks.
            let id = installation_id(&root)?;
            let (code, current) = tool("schtasks.exe", &["/Query", "/TN", AGENT_TASK, "/XML"])?;
            if code != 0
                || !task_belongs_to_installation(
                    &current,
                    &config.owner_sid,
                    &config.agent_image,
                    &id,
                )
            {
                return Err(fail(
                    "installation_mismatch",
                    "Agent task identity could not be verified",
                ));
            }
            let _ = tool("schtasks.exe", &["/End", "/TN", AGENT_TASK])?;
            wait_for_clients_exit(&config)?;
            checked_tool("schtasks.exe", &["/Delete", "/TN", AGENT_TASK, "/F"])?;
            crate::update_broker::remove_recovery(&root, &id)?;
            // Remove the task namespace only when it is empty. Other tasks are
            // never deleted merely because they share Focuser's folder name.
            checked_tool(
                r"WindowsPowerShell\v1.0\powershell.exe",
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    r"$ErrorActionPreference='Stop'; $s=New-Object -ComObject Schedule.Service; $s.Connect(); $f=$s.GetFolder('\Focuser'); if($f.GetTasks(1).Count -eq 0 -and $f.GetFolders(0).Count -eq 0){$s.GetFolder('\').DeleteFolder('Focuser',0)}",
                ],
            )?;
            if delete_data {
                // Validate the entire private tree before committing its purge.
                // No elevated traversal of user-controlled profile directories.
                let _inventory = focuser_ipc::cleanup::OwnedTree::inspect(&root, &verify_private)?;
            }
            service.delete().map_err(|e| fail("cleanup_failed", e))?;
            drop(service);
            progress(&root, transaction, "resources-removed", delete_data)?;
            progress(&root, transaction, "helper-complete", delete_data)?;
            Ok(())
        },
    )?;
    if delete_data {
        // Only after consumed authorization, agent cleanup, normal SCM shutdown
        // and owned resource removal. Release our own lock before deleting it.
        // Retained-data removal deliberately leaves enrollment and history intact.
        drop(_maintenance_lock);
        focuser_ipc::cleanup::OwnedTree::inspect(&root, &verify_private)?.remove()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storage_cleanup_waits_for_actual_process_exit_with_a_bounded_failure() {
        use ::windows::Win32::{Foundation::HANDLE, System::Threading::GetCurrentProcess};
        use std::os::windows::io::AsRawHandle;
        assert_eq!(
            wait_for_process_exit(unsafe { GetCurrentProcess() }, 0)
                .unwrap_err()
                .code,
            "maintenance_timeout"
        );
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/C", "exit", "0"])
            .spawn()
            .unwrap();
        wait_for_process_exit(HANDLE(child.as_raw_handle()), 5_000).unwrap();
        assert!(child.wait().unwrap().success());
    }
    #[test]
    fn logon_task_is_owner_bound_unprivileged_and_single_instance() {
        let t = agent_task(
            "S-1-5-21-1",
            Path::new(r"C:\Program Files\Focuser\agent.exe"),
            "install",
        );
        assert!(t.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(t.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(t.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(t.contains("<UserId>S-1-5-21-1</UserId>"));
        assert!(!t.contains("focuser-ui"));
        let rewritten = t
            .replace("id=\"Owner\"", "id=\"Author\"")
            .replace("Context=\"Owner\"", "Context=\"Author\"")
            .replace("</Command>", "</Command><Arguments> </Arguments>");
        assert!(task_belongs_to_installation(
            &rewritten,
            "S-1-5-21-1",
            Path::new(r"C:\Program Files\Focuser\agent.exe"),
            "install"
        ));
        assert!(task_belongs_to_installation(
            &t,
            "S-1-5-21-1",
            Path::new(r"C:\Program Files\Focuser\agent.exe"),
            "install"
        ));
        assert!(!task_belongs_to_installation(
            &t.replace("LeastPrivilege", "HighestAvailable"),
            "S-1-5-21-1",
            Path::new(r"C:\Program Files\Focuser\agent.exe"),
            "install"
        ));
        assert!(!task_belongs_to_installation(
            &t.replace(
                "</Actions>",
                "<Exec><Command>other.exe</Command></Exec></Actions>"
            ),
            "S-1-5-21-1",
            Path::new(r"C:\Program Files\Focuser\agent.exe"),
            "install"
        ));
        assert!(agent_task("x&y", Path::new("a<b"), "i").contains("x&amp;y"));
    }
}
