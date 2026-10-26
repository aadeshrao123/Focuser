//! Windows boundary. Every unsafe block owns/checks its Win32 handles; token
//! impersonation is confined to a synchronous function with no await points.
use crate::*;
use ::windows::Win32::{
    Foundation::*,
    Security::Authorization::*,
    Security::*,
    Storage::FileSystem::*,
    System::{Pipes::*, Services::*, Threading::*},
};
use ::windows::core::{PCWSTR, PWSTR};
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer};

pub const SYSTEM_SID: &str = "S-1-5-18";

pub struct OwnedHandle(pub HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn error(code: &str, e: impl std::fmt::Display) -> IpcError {
    IpcError::new(code, e.to_string())
}
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn validate_sid(sid: &str) -> Result<(), IpcError> {
    if !sid.starts_with("S-1-")
        || sid.len() > 180
        || !sid[4..].bytes().all(|c| c.is_ascii_digit() || c == b'-')
    {
        return Err(IpcError::new("configuration", "Invalid owner SID"));
    }
    let mut native = PSID::default();
    unsafe {
        ConvertStringSidToSidW(PCWSTR(wide(sid).as_ptr()), &mut native)
            .map_err(|e| error("configuration", e))?;
        let _ = LocalFree(HLOCAL(native.0));
    }
    Ok(())
}

unsafe fn sid_string(sid: PSID) -> Result<String, IpcError> {
    let mut text = PWSTR::null();
    unsafe {
        ConvertSidToStringSidW(sid, &mut text).map_err(|e| error("identity", e))?;
        let result = text.to_string().map_err(|e| error("identity", e));
        let _ = LocalFree(HLOCAL(text.0.cast()));
        result
    }
}

fn token_identity(token: HANDLE) -> Result<(String, u32), IpcError> {
    unsafe {
        let mut needed = 0;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        if needed == 0 || needed > 64 * 1024 {
            return Err(IpcError::new("identity", "Invalid token size"));
        }
        let mut storage = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        GetTokenInformation(
            token,
            TokenUser,
            Some(storage.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
        .map_err(|e| error("identity", e))?;
        let user = &*storage.as_ptr().cast::<TOKEN_USER>();
        let sid = sid_string(user.User.Sid)?;
        let mut session = 0u32;
        GetTokenInformation(
            token,
            TokenSessionId,
            Some((&mut session as *mut u32).cast()),
            4,
            &mut needed,
        )
        .map_err(|e| error("identity", e))?;
        Ok((sid, session))
    }
}

pub fn current_identity() -> Result<(String, u32), IpcError> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| error("identity", e))?;
        let token = OwnedHandle(token);
        token_identity(token.0)
    }
}

fn token_is_elevated(token: HANDLE) -> Result<bool, IpcError> {
    unsafe {
        let mut elevation = TOKEN_ELEVATION::default();
        let mut needed = 0;
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut needed,
        )
        .map_err(|e| error("identity", e))?;
        if elevation.TokenIsElevated == 0 {
            return Ok(false);
        }
        let _ = GetTokenInformation(token, TokenGroups, None, 0, &mut needed);
        if needed == 0 || needed > 64 * 1024 {
            return Err(IpcError::new("identity", "Invalid token groups"));
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        GetTokenInformation(
            token,
            TokenGroups,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
        .map_err(|e| error("identity", e))?;
        let groups = &*buffer.as_ptr().cast::<TOKEN_GROUPS>();
        for group in std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize)
        {
            if group.Attributes & 0x00000004 /* SE_GROUP_ENABLED */ != 0
                && sid_string(group.Sid)? == "S-1-5-32-544"
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
pub fn current_is_elevated() -> Result<bool, IpcError> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| error("identity", e))?;
        let token = OwnedHandle(token);
        token_is_elevated(token.0)
    }
}
pub fn require_elevated_process(pid: u32) -> Result<(), IpcError> {
    unsafe {
        let process = OwnedHandle(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .map_err(|e| error("unauthorized", e))?,
        );
        let mut token = HANDLE::default();
        OpenProcessToken(process.0, TOKEN_QUERY, &mut token)
            .map_err(|e| error("unauthorized", e))?;
        let token = OwnedHandle(token);
        if !token_is_elevated(token.0)? {
            return Err(IpcError::new(
                "elevation_required",
                "An elevated administrator token is required",
            ));
        }
        Ok(())
    }
}

fn image(handle: HANDLE) -> Result<PathBuf, IpcError> {
    let mut text = vec![0u16; 32768];
    let mut size = text.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(text.as_mut_ptr()),
            &mut size,
        )
    }
    .map_err(|e| error("identity", e))?;
    Ok(PathBuf::from(String::from_utf16_lossy(
        &text[..size as usize],
    )))
}

pub fn process_identity(pid: u32) -> Result<Peer, IpcError> {
    unsafe {
        let process = OwnedHandle(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .map_err(|e| error("identity", e))?,
        );
        let mut token = HANDLE::default();
        OpenProcessToken(process.0, TOKEN_QUERY, &mut token).map_err(|e| error("identity", e))?;
        let token = OwnedHandle(token);
        let (sid, session) = token_identity(token.0)?;
        Ok(Peer {
            sid,
            session,
            pid,
            image: image(process.0)?,
        })
    }
}

/// Evaluate and terminate the SAME open process handle, avoiding a PID-reuse
/// race between checking the executable/owner and applying an app rule.
pub fn enforce_process(
    pid: u32,
    owner: &str,
    session: Option<u32>,
    decide: impl FnOnce(&Peer) -> bool,
) -> Result<bool, IpcError> {
    if pid <= 4 || pid == std::process::id() {
        return Ok(false);
    }
    unsafe {
        let process = OwnedHandle(
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                false,
                pid,
            )
            .map_err(|e| error("identity", e))?,
        );
        let mut token = HANDLE::default();
        OpenProcessToken(process.0, TOKEN_QUERY, &mut token).map_err(|e| error("identity", e))?;
        let token = OwnedHandle(token);
        let (sid, actual_session) = token_identity(token.0)?;
        if sid != owner || session.is_some_and(|expected| expected != actual_session) {
            return Ok(false);
        }
        let peer = Peer {
            sid,
            session: actual_session,
            pid,
            image: image(process.0)?,
        };
        if !decide(&peer) {
            return Ok(false);
        }
        TerminateProcess(process.0, 1).map_err(|e| error("enforcement", e))?;
        Ok(true)
    }
}

struct Impersonation;
impl Drop for Impersonation {
    fn drop(&mut self) {
        // Never continue a worker thread with an unexpected security token.
        if unsafe { RevertToSelf() }.is_err() {
            std::process::abort();
        }
    }
}

/// Must be called AFTER reading the frame (the pipe impersonates the sender
/// of the last message read). Never use PID/image in place of token identity.
pub fn authenticate_client(pipe: &NamedPipeServer, owner_sid: &str) -> Result<Peer, IpcError> {
    unsafe {
        let handle = HANDLE(pipe.as_raw_handle());
        ImpersonateNamedPipeClient(handle).map_err(|e| error("unauthorized", e))?;
        let impersonation = Impersonation;
        let mut token = HANDLE::default();
        OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut token)
            .map_err(|e| error("unauthorized", e))?;
        let token = OwnedHandle(token);
        let (sid, session) = token_identity(token.0)?;
        let elevated = token_is_elevated(token.0)?;
        drop(token);
        drop(impersonation);
        if sid != owner_sid && sid != SYSTEM_SID && !elevated {
            return Err(IpcError::new(
                "unauthorized",
                "The pipe token is not the configured policy owner",
            ));
        }
        let mut pid = 0;
        GetNamedPipeClientProcessId(handle, &mut pid).map_err(|e| error("unauthorized", e))?;
        let peer = process_identity(pid)?;
        // PID metadata is corroborating information only, never proof of identity.
        if peer.sid != sid || peer.session != session {
            return Err(IpcError::new(
                "unauthorized",
                "Pipe token/process identity mismatch",
            ));
        }
        Ok(peer)
    }
}

pub struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(HLOCAL(self.0.0));
        }
    }
}
impl Descriptor {
    pub fn from_sddl(sddl: &str) -> Result<Self, IpcError> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide(sddl).as_ptr()),
                1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|e| error("security", e))?;
        Ok(Self(descriptor))
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        }
    }
    pub fn pointer(&self) -> PSECURITY_DESCRIPTOR {
        self.0
    }
}

pub fn create_server(
    name: &str,
    owner_sid: &str,
    production: bool,
    first: bool,
) -> Result<NamedPipeServer, IpcError> {
    validate_sid(owner_sid)?;
    // Do not grant FILE_CREATE_PIPE_INSTANCE (0x4) to production clients.
    let sddl = if production {
        format!(
            "O:SYG:SYD:P(D;;GA;;;NU)(A;;FA;;;SY)(A;;0x00120183;;;BA)(A;;0x00120183;;;{owner_sid})"
        )
    } else {
        format!("O:{owner_sid}D:P(D;;GA;;;NU)(A;;FA;;;SY)(A;;FA;;;{owner_sid})")
    };
    let descriptor = Descriptor::from_sddl(&sddl)?;
    let attributes = descriptor.attributes();
    let mut flags = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        flags |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(wide(name).as_ptr()),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            MAX_CONNECTIONS as u32 + 1,
            64 * 1024,
            64 * 1024,
            5000,
            Some(&attributes),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(error("transport", ::windows::core::Error::from_win32()));
    }
    // Tokio takes ownership of this overlapped handle, including on failure.
    unsafe { NamedPipeServer::from_raw_handle(handle.0) }.map_err(|e| error("transport", e))
}

fn scm_pid() -> Result<u32, IpcError> {
    unsafe {
        let manager =
            OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).map_err(|_| {
                IpcError::new(
                    "service_unavailable",
                    "Focuser authority is not installed or cannot be queried",
                )
            })?;
        let service = OpenServiceW(
            manager,
            PCWSTR(wide(SERVICE_NAME).as_ptr()),
            SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        );
        let _ = CloseServiceHandle(manager);
        let service = service.map_err(|_| {
            IpcError::new("service_unavailable", "Focuser authority is not installed")
        })?;
        let mut status = SERVICE_STATUS_PROCESS::default();
        let bytes = std::slice::from_raw_parts_mut(
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
            size_of::<SERVICE_STATUS_PROCESS>(),
        );
        let result = QueryServiceStatusEx(service, SC_STATUS_PROCESS_INFO, Some(bytes), &mut 0);
        let configured = (|| {
            result.map_err(|e| error("server_identity", e))?;
            let mut needed = 0;
            let _ = QueryServiceConfigW(service, None, 0, &mut needed);
            if needed == 0 || needed > 64 * 1024 {
                return Err(IpcError::new(
                    "server_identity",
                    "Invalid SCM configuration size",
                ));
            }
            let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
            let config = buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
            QueryServiceConfigW(service, Some(config), needed, &mut needed)
                .map_err(|e| error("server_identity", e))?;
            let account = (*config)
                .lpServiceStartName
                .to_string()
                .map_err(|e| error("server_identity", e))?;
            if !account.eq_ignore_ascii_case("LocalSystem")
                && !account.eq_ignore_ascii_case(r"NT AUTHORITY\SYSTEM")
            {
                return Err(IpcError::new(
                    "server_identity",
                    "SCM authority account is not LocalSystem",
                ));
            }
            Ok(())
        })();
        let _ = CloseServiceHandle(service);
        configured?;
        if status.dwCurrentState != SERVICE_RUNNING || status.dwProcessId <= 4 {
            return Err(IpcError::new(
                "service_unavailable",
                "Focuser authority is not running",
            ));
        }
        Ok(status.dwProcessId)
    }
}

fn authenticate_server(pipe: &NamedPipeClient, endpoint: &Endpoint) -> Result<(), IpcError> {
    unsafe {
        let handle = HANDLE(pipe.as_raw_handle());
        let mut pid = 0;
        GetNamedPipeServerProcessId(handle, &mut pid).map_err(|e| error("server_identity", e))?;
        let (expected_pid, expected_owner) = match endpoint {
            Endpoint::Production => (scm_pid()?, SYSTEM_SID.to_owned()),
            Endpoint::Development { server_pid, .. } => (*server_pid, current_identity()?.0),
        };
        if pid != expected_pid {
            return Err(IpcError::new(
                "server_identity",
                "Pipe server is not the expected authority process",
            ));
        }
        // A standard user need not have TOKEN_QUERY on a SYSTEM process.
        // Production instead authenticates the SCM PID/account AND the actual
        // SYSTEM-owned kernel pipe object below; startup checks its own token.
        // Development has no SCM trust anchor, so it requires the token check.
        if matches!(endpoint, Endpoint::Development { .. })
            && process_identity(pid)?.sid != expected_owner
        {
            return Err(IpcError::new(
                "server_identity",
                "Authority token identity does not match",
            ));
        }
        // Unlike a claimed PID, a user cannot assign SYSTEM as an object's owner.
        let mut owner = PSID::default();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            Some(&mut descriptor),
        )
        .ok()
        .map_err(|e| error("server_identity", e))?;
        let actual_owner = sid_string(owner);
        let _ = LocalFree(HLOCAL(descriptor.0));
        if actual_owner? != expected_owner {
            return Err(IpcError::new(
                "server_identity",
                "Untrusted pipe object owner",
            ));
        }
        Ok(())
    }
}

pub async fn request(endpoint: &Endpoint, body: RequestBody) -> Result<ReplyBody, IpcError> {
    let name = endpoint.pipe_name()?;
    let until = tokio::time::Instant::now() + IO_TIMEOUT;
    let mut pipe = loop {
        // Identification-only prevents an impostor server from using the
        // client's token to perform filesystem/network operations as that user.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide(&name).as_ptr()),
                0x00120183,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED
                    | FILE_FLAGS_AND_ATTRIBUTES(
                        SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0,
                    ),
                HANDLE::default(),
            )
        };
        match handle {
            Ok(handle) => {
                break unsafe { NamedPipeClient::from_raw_handle(handle.0) }
                    .map_err(|e| error("transport", e))?;
            }
            Err(_) if tokio::time::Instant::now() < until => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await
            }
            Err(_) => {
                return Err(IpcError::new(
                    "service_unavailable",
                    "Cannot reach the Focuser authority; no local database fallback was started",
                ));
            }
        }
    };
    authenticate_server(&pipe, endpoint)?;
    write_frame(
        &mut pipe,
        &Request {
            version: VERSION,
            body,
        },
    )
    .await?;
    let reply: Reply = read_frame(&mut pipe).await?;
    use tokio::io::AsyncWriteExt;
    let _ = tokio::time::timeout(IO_TIMEOUT, pipe.write_all(&[REPLY_ACK])).await;
    if reply.version != VERSION {
        return Err(IpcError::new(
            "version_mismatch",
            "Authority protocol version differs",
        ));
    }
    reply.result
}
