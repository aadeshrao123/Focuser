//! Experimental kernel transport. Ordinary builds never connect to or install a
//! driver. A kernel-file-guard build requires the enrolled kernel endpoint; it
//! must not fall back to an unguarded production engine.
use focuser_common::types::BlockList;
use focuser_ipc::IpcError;

const MAGIC: u32 = 0x46434744;
const VERSION: u16 = 1;
const MESSAGE_SIZE: usize = 80;
const REPLY_SIZE: usize = 56;
const BEGIN: u32 = 1;
const COMMIT: u32 = 2;
const QUERY: u32 = 3;
const PREPARE_REMOVAL: u32 = 4;
const CONSUME_REMOVAL: u32 = 5;

fn failure(code: &str, message: impl std::fmt::Display) -> IpcError {
    IpcError::new(code, message.to_string())
}

/// Same stored commitment decision as uninstall: breaks and pauses never become
/// an unlock, and no arbitrary heartbeat lease extends the original deadline.
pub fn deadline(lists: &[BlockList]) -> Result<u64, IpcError> {
    let decision = crate::removal::decision(lists);
    if decision.allowed {
        return Ok(0);
    }
    match decision.release_at {
        None => Ok(u64::MAX),
        Some(value) => chrono::DateTime::parse_from_rfc3339(&value)
            .ok()
            .and_then(|v| {
                u64::try_from(v.timestamp()).ok().and_then(|seconds| {
                    seconds
                        .checked_mul(10_000_000)?
                        .checked_add(u64::from(v.timestamp_subsec_nanos()).div_ceil(100))
                })
            })
            .ok_or_else(|| failure("kernel_guard_policy", "Invalid stored release time")),
    }
}

pub trait Transport: Send {
    fn exchange(&mut self, message: [u8; MESSAGE_SIZE]) -> Result<[u8; REPLY_SIZE], IpcError>;
}

pub struct Controller {
    transport: Box<dyn Transport>,
    installation: [u8; 16],
    sequence: u64,
    revision: u64,
    flags: u32,
    healthy: bool,
}

impl Controller {
    pub fn new(transport: Box<dyn Transport>, installation: [u8; 16]) -> Self {
        Self {
            transport,
            installation,
            sequence: 0,
            revision: 0,
            flags: 0,
            healthy: true,
        }
    }

    fn request(
        &mut self,
        operation: u32,
        revision: u64,
        until: u64,
        permit: [u8; 16],
    ) -> Result<(), IpcError> {
        if !self.healthy {
            return Err(failure(
                "kernel_guard_disconnected",
                "Kernel protection requires service recovery",
            ));
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| failure("kernel_guard_sequence", "Sequence exhausted"))?;
        let mut message = [0u8; MESSAGE_SIZE];
        message[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        message[4..6].copy_from_slice(&VERSION.to_le_bytes());
        message[6..8].copy_from_slice(&(MESSAGE_SIZE as u16).to_le_bytes());
        message[8..12].copy_from_slice(&operation.to_le_bytes());
        message[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        message[24..32].copy_from_slice(&revision.to_le_bytes());
        message[32..40].copy_from_slice(&until.to_le_bytes());
        message[40..56].copy_from_slice(&self.installation);
        message[56..72].copy_from_slice(&permit);
        let reply = match self.transport.exchange(message) {
            Ok(reply) => reply,
            Err(error) => {
                self.healthy = false;
                return Err(error);
            }
        };
        let u16_at = |at| u16::from_le_bytes(reply[at..at + 2].try_into().unwrap());
        let u32_at = |at| u32::from_le_bytes(reply[at..at + 4].try_into().unwrap());
        let u64_at = |at| u64::from_le_bytes(reply[at..at + 8].try_into().unwrap());
        if u32_at(0) != MAGIC
            || u16_at(4) != VERSION
            || u16_at(6) != REPLY_SIZE as u16
            || u64_at(16) != self.sequence
            || reply[40..56] != self.installation
            || u32_at(12) & !7 != 0
            || (u32_at(8) == 0 && operation != QUERY && u64_at(24) != revision)
            || (u32_at(8) == 0 && operation == COMMIT && u64_at(32) != until)
        {
            self.healthy = false;
            return Err(failure(
                "kernel_guard_protocol",
                "Malformed or mismatched kernel reply",
            ));
        }
        self.flags = u32_at(12);
        self.revision = u64_at(24);
        match u32_at(8) {
            0 => Ok(()),
            code => Err(failure(
                match code {
                    1 => "kernel_guard_protocol",
                    2 => "kernel_guard_identity",
                    3 => "kernel_guard_replay",
                    4 => "kernel_guard_revision",
                    5 => "removal_protected",
                    6 => "kernel_guard_permit",
                    7 => "kernel_guard_busy",
                    _ => "kernel_guard_unknown",
                },
                format!("Kernel guard refused operation {operation} (code {code})"),
            )),
        }
    }

    pub fn begin_change(&mut self) -> Result<(), IpcError> {
        self.request(BEGIN, self.revision, 0, [0; 16])
    }
    pub fn synchronize(
        &mut self,
        installation: &str,
        revision: u64,
        lists: &[BlockList],
    ) -> Result<(), IpcError> {
        if uuid::Uuid::parse_str(installation)
            .ok()
            .map(|v| *v.as_bytes())
            != Some(self.installation)
        {
            return Err(failure(
                "kernel_guard_identity",
                "Database and driver enrollment do not match",
            ));
        }
        let until = deadline(lists)?;
        self.begin_change()?;
        self.request(COMMIT, revision, until, [0; 16])
    }
    /// Called only after the existing authority consumes and rechecks its permit.
    pub fn authorize_removal(&mut self, transaction: &str, revision: u64) -> Result<(), IpcError> {
        let permit = *uuid::Uuid::parse_str(transaction)
            .map_err(|e| failure("kernel_guard_permit", e))?
            .as_bytes();
        self.request(PREPARE_REMOVAL, revision, 0, permit)?;
        self.request(CONSUME_REMOVAL, revision, 0, permit)
    }
    pub fn status(&self) -> &'static str {
        if !self.healthy {
            "disconnected"
        } else if self.flags & 2 != 0 {
            "transition_pending"
        } else if self.flags & 4 != 0 {
            "removal_authorized"
        } else if self.flags & 1 != 0 {
            "experimental_protected"
        } else {
            "experimental_idle"
        }
    }
}

#[cfg(all(windows, feature = "kernel-file-guard"))]
pub fn connect() -> Result<Controller, IpcError> {
    use winreg::{RegKey, enums::*};
    // A normal unsigned development service cannot control the production
    // driver. This reuses the existing release publisher pin and trust check.
    let trust = crate::update::Trust::production()?;
    crate::update_broker::verify_authenticode(
        &std::env::current_exe().map_err(|e| failure("identity", e))?,
        &trust.publisher,
    )?;
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SYSTEM\CurrentControlSet\Services\FocuserGuard\Parameters",
            KEY_READ,
        )
        .map_err(|e| failure("kernel_guard_unavailable", e))?;
    let raw = key
        .get_raw_value("Installation")
        .map_err(|e| failure("kernel_guard_identity", e))?;
    if raw.vtype != REG_BINARY || raw.bytes.len() != 16 || raw.bytes.iter().all(|b| *b == 0) {
        return Err(failure(
            "kernel_guard_identity",
            "Invalid driver enrollment",
        ));
    }
    let mut controller = Controller::new(
        Box::new(native::Port::connect()?),
        raw.bytes.try_into().unwrap(),
    );
    controller.request(QUERY, 0, 0, [0; 16])?;
    Ok(controller)
}

#[cfg(all(windows, feature = "kernel-file-guard"))]
mod native {
    use super::*;
    use std::sync::mpsc::{self, SyncSender};
    use std::time::Duration;
    use windows::{Win32::Storage::InstallableFileSystems::*, core::w};
    const TIMEOUT: Duration = Duration::from_secs(3);
    type ResultReply = Result<[u8; REPLY_SIZE], IpcError>;
    type Work = ([u8; MESSAGE_SIZE], SyncSender<ResultReply>);
    pub struct Port {
        sender: SyncSender<Work>,
    }
    impl Port {
        pub fn connect() -> Result<Self, IpcError> {
            let (ready_tx, ready_rx) = mpsc::sync_channel(0);
            let (sender, receiver) = mpsc::sync_channel::<Work>(1);
            std::thread::Builder::new()
                .name("kernel-guard-port".into())
                .spawn(move || {
                    let handle = match unsafe {
                        FilterConnectCommunicationPort(w!("\\FocuserGuard.v1"), 0, None, 0, None)
                    } {
                        Ok(handle) => focuser_ipc::windows::OwnedHandle(handle),
                        Err(e) => {
                            let _ = ready_tx.send(Err(failure("kernel_guard_unavailable", e)));
                            return;
                        }
                    };
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    while let Ok((message, reply_tx)) = receiver.recv() {
                        let mut reply = [0u8; REPLY_SIZE];
                        let mut returned = 0;
                        let result = unsafe {
                            FilterSendMessage(
                                handle.0,
                                message.as_ptr().cast(),
                                MESSAGE_SIZE as u32,
                                Some(reply.as_mut_ptr().cast()),
                                REPLY_SIZE as u32,
                                &mut returned,
                            )
                        }
                        .map_err(|e| failure("kernel_guard_disconnected", e))
                        .and_then(|_| {
                            if returned as usize == REPLY_SIZE {
                                Ok(reply)
                            } else {
                                Err(failure(
                                    "kernel_guard_protocol",
                                    "Incorrect kernel reply length",
                                ))
                            }
                        });
                        let _ = reply_tx.send(result);
                    }
                })
                .map_err(|e| failure("kernel_guard_worker", e))?;
            ready_rx
                .recv_timeout(TIMEOUT)
                .map_err(|e| failure("kernel_guard_timeout", e))??;
            Ok(Self { sender })
        }
    }
    impl Transport for Port {
        fn exchange(&mut self, message: [u8; MESSAGE_SIZE]) -> ResultReply {
            self.exchange_with_timeout(message, TIMEOUT)
        }
    }
    impl Port {
        fn exchange_with_timeout(
            &mut self,
            message: [u8; MESSAGE_SIZE],
            timeout: Duration,
        ) -> ResultReply {
            let (sender, receiver) = mpsc::sync_channel(1);
            self.sender
                .try_send((message, sender))
                .map_err(|e| failure("kernel_guard_busy", e))?;
            receiver
                .recv_timeout(timeout)
                .map_err(|e| failure("kernel_guard_timeout", e))?
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn an_unresponsive_worker_has_a_bounded_queue_and_diagnostic_timeout() {
            let (sender, receiver) = mpsc::sync_channel(1);
            let mut port = Port { sender };
            assert_eq!(
                port.exchange_with_timeout([0; MESSAGE_SIZE], Duration::from_millis(5))
                    .unwrap_err()
                    .code,
                "kernel_guard_timeout"
            );
            assert_eq!(
                port.exchange_with_timeout([0; MESSAGE_SIZE], Duration::from_millis(5))
                    .unwrap_err()
                    .code,
                "kernel_guard_busy"
            );
            assert!(receiver.try_recv().is_ok());
            assert!(receiver.try_recv().is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Echo {
        corrupt: bool,
        fail: bool,
    }
    impl Transport for Echo {
        fn exchange(&mut self, m: [u8; MESSAGE_SIZE]) -> Result<[u8; REPLY_SIZE], IpcError> {
            if self.fail {
                return Err(failure("kernel_guard_unavailable", "Absent"));
            }
            assert_eq!(&m[12..16], &[0; 4]);
            assert_eq!(&m[72..80], &[0; 8]);
            let mut r = [0; REPLY_SIZE];
            r[..8].copy_from_slice(&m[..8]);
            r[6..8].copy_from_slice(&(REPLY_SIZE as u16).to_le_bytes());
            r[16..56].copy_from_slice(&m[16..56]);
            if self.corrupt {
                r[40] ^= 1;
            }
            Ok(r)
        }
    }
    #[test]
    fn framing_and_identity_are_checked_and_no_replacement_transport_is_started() {
        let id = uuid::Uuid::new_v4();
        let mut c = Controller::new(
            Box::new(Echo {
                corrupt: false,
                fail: false,
            }),
            *id.as_bytes(),
        );
        c.synchronize(&id.to_string(), 4, &[]).unwrap();
        c.authorize_removal(&uuid::Uuid::new_v4().to_string(), 4)
            .unwrap();
        assert_eq!(c.revision, 4);
        let mut c = Controller::new(
            Box::new(Echo {
                corrupt: true,
                fail: false,
            }),
            *id.as_bytes(),
        );
        assert_eq!(c.begin_change().unwrap_err().code, "kernel_guard_protocol");
        assert_eq!(
            c.begin_change().unwrap_err().code,
            "kernel_guard_disconnected"
        );
        let mut c = Controller::new(
            Box::new(Echo {
                corrupt: false,
                fail: true,
            }),
            *id.as_bytes(),
        );
        assert_eq!(
            c.begin_change().unwrap_err().code,
            "kernel_guard_unavailable"
        );
        assert_eq!(c.status(), "disconnected");
        assert_eq!(
            c.begin_change().unwrap_err().code,
            "kernel_guard_disconnected"
        );
    }

    #[test]
    fn deadlines_follow_overlapping_commitments_and_ignore_temporary_disabling() {
        use focuser_common::types::{Lock, Protection};
        let mut first = BlockList::new("First");
        first.protection = Some(Protection::for_duration(600));
        let mut second = BlockList::new("Second");
        second.protection = Some(Protection::for_duration(1200));
        let expiry = second.protection.as_ref().unwrap().expires_at.unwrap();
        let expected = expiry.timestamp() as u64 * 10_000_000
            + u64::from(expiry.timestamp_subsec_nanos()).div_ceil(100);
        assert_eq!(
            deadline(&[first.clone(), second.clone()]).unwrap(),
            expected
        );
        second.enabled = false; // Temporary engine/break state cannot unlock a commitment.
        assert_eq!(
            deadline(&[first.clone(), second.clone()]).unwrap(),
            expected
        );
        second.protection.as_mut().unwrap().expires_at =
            Some(chrono::Utc::now() - chrono::Duration::seconds(1));
        let expiry = first.protection.as_ref().unwrap().expires_at.unwrap();
        assert_eq!(
            deadline(&[first.clone(), second]).unwrap(),
            expiry.timestamp() as u64 * 10_000_000
                + u64::from(expiry.timestamp_subsec_nanos()).div_ceil(100)
        );
        first.protection = None; // The app performs verified unlock before this snapshot.
        assert_eq!(deadline(&[first.clone()]).unwrap(), 0);
        first.lock = Some(Lock::password("local test fixture").unwrap());
        first.protection = Some(Protection::until_unlocked());
        assert_eq!(deadline(&[first]).unwrap(), u64::MAX);
    }
}
