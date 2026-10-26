//! Removal evaluates stored commitments independently of temporary blocking
//! pauses. It never rewrites their flags, unlock conditions or original expiry.
use focuser_common::types::BlockList;
use focuser_ipc::*;
use std::time::{Duration, Instant};

pub fn decision(lists: &[BlockList]) -> RemovalDecision {
    decision_at(lists, chrono::Local::now())
}
/// Erasing all data additionally requires every running block to have ended.
/// This explicit mode does not change legacy retained-data uninstall opt-outs.
pub fn complete_decision_at<T: chrono::TimeZone>(
    lists: &[BlockList],
    now: chrono::DateTime<T>,
) -> RemovalDecision {
    let mut result = decision_at(lists, now.clone());
    for list in lists {
        if result
            .blockers
            .iter()
            .any(|b| b.list_id == list.id.to_string())
        {
            continue;
        }
        let commitment = list.manual_protection_at(now.with_timezone(&chrono::Utc));
        let running = list.enabled
            && list.schedule.as_ref().is_none_or(|s| {
                s.time_slots.is_empty() || s.active_period_at(now.clone()).is_some()
            });
        if commitment.is_none()
            && !(running
                && (list.websites.iter().any(|r| r.enabled)
                    || list.applications.iter().any(|r| r.enabled)))
        {
            continue;
        }
        result.blockers.push(RemovalBlocker {
            list_id: list.id.to_string(),
            name: list.name.clone(),
            reason: "running_block".into(),
            release_at: match commitment {
                Some(p) => p.expires_at,
                None => list.occurrence_at(now.clone()).map(|(_, end)| end),
            }
            .map(|t| t.to_rfc3339()),
        });
    }
    result.allowed = result.blockers.is_empty();
    result.code = if result.allowed {
        "removal_allowed"
    } else {
        "removal_protected"
    }
    .into();
    result.release_at = if result.blockers.iter().any(|b| b.release_at.is_none()) {
        None
    } else {
        result
            .blockers
            .iter()
            .filter_map(|b| b.release_at.clone())
            .max()
    };
    result
}
pub fn decision_at<T: chrono::TimeZone>(
    lists: &[BlockList],
    now: chrono::DateTime<T>,
) -> RemovalDecision {
    let blockers: Vec<_> = lists
        .iter()
        .filter_map(|l| {
            let p = l.effective_protection_at(now.clone())?;
            if !p.prevent_uninstall {
                return None;
            }
            let manual = l.manual_protection_at(now.with_timezone(&chrono::Utc));
            let scheduled = l.scheduled_protection_at(now.clone());
            let relevant: Vec<_> = [manual, scheduled.as_ref()]
                .into_iter()
                .flatten()
                .filter(|p| p.prevent_uninstall)
                .collect();
            let release_at = if relevant.iter().any(|p| p.expires_at.is_none()) {
                None
            } else {
                relevant
                    .iter()
                    .filter_map(|p| p.expires_at)
                    .max()
                    .map(|t| t.to_rfc3339())
            };
            Some(RemovalBlocker {
                list_id: l.id.to_string(),
                name: l.name.clone(),
                reason: if l
                    .manual_protection_at(now.with_timezone(&chrono::Utc))
                    .is_some_and(|p| p.prevent_uninstall)
                {
                    "manual_commitment"
                } else {
                    "scheduled_occurrence"
                }
                .into(),
                release_at,
            })
        })
        .collect();
    let release_at = if blockers.iter().any(|b| b.release_at.is_none()) {
        None
    } else {
        blockers.iter().filter_map(|b| b.release_at.clone()).max()
    };
    RemovalDecision {
        shutdown_clients: false,
        allowed: blockers.is_empty(),
        code: if blockers.is_empty() {
            "removal_allowed"
        } else {
            "removal_protected"
        }
        .into(),
        blockers,
        release_at,
    }
}

/// Plain text for the installed helper and interactive/silent installer details.
/// Names are stored policy data, never client assertions of unlock authority.
pub fn explanation(decision: &RemovalDecision) -> String {
    if decision.allowed {
        return "Focuser can be uninstalled; no uninstall commitments remain.".into();
    }
    let mut message =
        "Focuser cannot be uninstalled while these commitments are protected:".to_string();
    for blocker in &decision.blockers {
        let name: String = blocker.name.chars().filter(|c| !c.is_control()).collect();
        let release = blocker.release_at.as_deref().map_or_else(
            || {
                if blocker.reason == "running_block" {
                    "until the block is ended in Focuser (unlock it first if required)".to_string()
                } else if blocker.reason == "pomodoro_session" {
                    "until the Pomodoro session is completed or ended in Focuser".to_string()
                } else {
                    "until it is legitimately unlocked in Focuser".to_string()
                }
            },
            |at| format!("until {at}"),
        );
        message.push_str(&format!("\n- {name}: {release}."));
    }
    message
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    sid: String,
    pid: u32,
    session: u32,
    image: std::path::PathBuf,
}
impl Caller {
    pub fn pid(&self) -> u32 {
        self.pid
    }
}
impl From<&Peer> for Caller {
    fn from(p: &Peer) -> Self {
        Self {
            sid: p.sid.clone(),
            pid: p.pid,
            session: p.session,
            image: p.image.clone(),
        }
    }
}
struct Permit {
    token: String,
    caller: Caller,
    revision: u64,
    delete_data: bool,
    expires: Instant,
}
pub struct Removal {
    pub installation: String,
    pub revision: u64,
    permit: Option<Permit>,
    pub transaction: Option<(String, Caller, bool)>,
    pub ready: bool,
    pub started: Instant,
    pub interrupted: bool,
    pub acknowledgements: std::collections::HashSet<(u32, u32)>,
}
impl Removal {
    pub fn new(installation: String, interrupted: bool) -> Self {
        Self {
            installation,
            revision: 0,
            permit: None,
            transaction: None,
            ready: false,
            interrupted,
            started: Instant::now(),
            acknowledgements: Default::default(),
        }
    }
    pub fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn issue(
        &mut self,
        caller: Caller,
        delete_data: bool,
        now: Instant,
    ) -> Result<MaintenanceReply, IpcError> {
        if self.transaction.is_some() {
            return Err(IpcError::new(
                "maintenance_busy",
                "Removal already in progress",
            ));
        }
        let token = uuid::Uuid::new_v4().to_string();
        self.permit = Some(Permit {
            token: token.clone(),
            caller,
            revision: self.revision,
            delete_data,
            expires: now + Duration::from_secs(15),
        });
        Ok(self.reply(token, "prepared"))
    }
    pub fn consume(
        &mut self,
        caller: &Caller,
        token: &str,
        delete_data: bool,
        now: Instant,
    ) -> Result<(), IpcError> {
        // Take before checking: even failed use burns the permit.
        let p = self
            .permit
            .take()
            .ok_or_else(|| IpcError::new("permit_invalid", "No unused permit"))?;
        if p.token != token || &p.caller != caller || p.delete_data != delete_data {
            return Err(IpcError::new(
                "permit_mismatch",
                "Permit is bound to installation, removal, caller and retention choice",
            ));
        }
        if now >= p.expires {
            return Err(IpcError::new("permit_expired", "Removal permit expired"));
        }
        if p.revision != self.revision {
            return Err(IpcError::new(
                "policy_changed",
                "Policy changed; request authorization again",
            ));
        }
        self.transaction = Some((token.into(), caller.clone(), delete_data));
        self.acknowledgements.clear();
        self.ready = false;
        self.started = now;
        Ok(())
    }
    pub fn require_transaction(&self, caller: &Caller, token: &str) -> Result<(), IpcError> {
        if !self
            .transaction
            .as_ref()
            .is_some_and(|(id, c, _)| id == token && c == caller)
        {
            return Err(IpcError::new(
                "permit_invalid",
                "Unknown or mismatched removal transaction",
            ));
        }
        Ok(())
    }
    pub fn reply(&self, transaction: String, phase: &str) -> MaintenanceReply {
        MaintenanceReply {
            installation: self.installation.clone(),
            transaction,
            phase: phase.into(),
        }
    }
    pub fn resume_if_abandoned(&mut self, now: Instant, caller_alive: bool) -> bool {
        if self.active()
            && (!caller_alive || now.duration_since(self.started) >= Duration::from_secs(120))
        {
            self.transaction = None;
            self.ready = false;
            self.interrupted = true;
            return true;
        }
        false
    }
    pub fn active(&self) -> bool {
        self.transaction.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn caller(pid: u32) -> Caller {
        Caller::from(&Peer {
            sid: "S-1-5-21-1".into(),
            pid,
            session: 1,
            image: "installed.exe".into(),
        })
    }
    #[test]
    fn permits_are_single_use_bound_expiring_and_revision_checked() {
        let now = Instant::now();
        let mut r = Removal::new("install-a".into(), false);
        let p = r.issue(caller(10), false, now).unwrap();
        assert_eq!(
            r.consume(&caller(11), &p.transaction, false, now)
                .unwrap_err()
                .code,
            "permit_mismatch"
        );
        assert_eq!(
            r.consume(&caller(10), &p.transaction, false, now)
                .unwrap_err()
                .code,
            "permit_invalid"
        );
        let p = r.issue(caller(10), false, now).unwrap();
        assert_eq!(
            r.consume(&caller(10), &p.transaction, true, now)
                .unwrap_err()
                .code,
            "permit_mismatch"
        );
        let p = r.issue(caller(10), false, now).unwrap();
        assert_eq!(
            r.consume(
                &caller(10),
                &p.transaction,
                false,
                now + Duration::from_secs(15)
            )
            .unwrap_err()
            .code,
            "permit_expired"
        );
        let p = r.issue(caller(10), false, now).unwrap();
        r.changed();
        assert_eq!(
            r.consume(&caller(10), &p.transaction, false, now)
                .unwrap_err()
                .code,
            "policy_changed"
        );
        let p = r.issue(caller(10), false, now).unwrap();
        r.consume(&caller(10), &p.transaction, false, now).unwrap();
        assert_eq!(
            r.consume(&caller(10), &p.transaction, false, now)
                .unwrap_err()
                .code,
            "permit_invalid"
        );
        assert!(r.require_transaction(&caller(11), &p.transaction).is_err());
        let mut restarted = Removal::new("install-a".into(), true);
        assert_eq!(
            restarted
                .consume(&caller(10), &p.transaction, false, now)
                .unwrap_err()
                .code,
            "permit_invalid"
        );
    }
    #[test]
    fn abandoned_helper_resumes_enforcement_and_cross_installation_permits_are_invalid() {
        let now = Instant::now();
        let mut r = Removal::new("installation-a".into(), false);
        let p = r.issue(caller(10), false, now).unwrap();
        let mut other = Removal::new("installation-b".into(), false);
        assert_eq!(
            other
                .consume(&caller(10), &p.transaction, false, now)
                .unwrap_err()
                .code,
            "permit_invalid"
        );
        r.consume(&caller(10), &p.transaction, false, now).unwrap();
        assert!(!r.resume_if_abandoned(now, true));
        assert!(r.active());
        assert!(r.resume_if_abandoned(now, false));
        assert!(!r.active());
        assert!(r.interrupted);
        assert!(!r.ready);
        let mut stalled = Removal::new("installation-a".into(), false);
        let p = stalled.issue(caller(10), false, now).unwrap();
        stalled
            .consume(&caller(10), &p.transaction, false, now)
            .unwrap();
        stalled.ready = true;
        assert!(stalled.resume_if_abandoned(now + Duration::from_secs(120), true));
        assert!(!stalled.ready);
        assert!(stalled.interrupted);
    }
    #[test]
    fn overlapping_commitments_include_temporarily_disabled_lists() {
        use focuser_common::types::Protection;
        let mut a = BlockList::new("a");
        a.protection = Some(Protection::for_duration(10));
        let mut b = BlockList::new("b");
        b.protection = Some(Protection::for_duration(20));
        let d = decision(&[a.clone(), b.clone()]);
        assert!(!d.allowed);
        assert_eq!(d.blockers.len(), 2);
        assert_eq!(
            d.release_at,
            b.protection
                .as_ref()
                .unwrap()
                .expires_at
                .map(|t| t.to_rfc3339())
        );
        a.enabled = false;
        b.protection.as_mut().unwrap().prevent_uninstall = false;
        assert!(!decision(&[a.clone(), b.clone()]).allowed);
        a.protection = None; // Expiry or verified unlock, not disabling the list.
        assert!(decision(&[a, b]).allowed);
    }
}
/// Shared by the installed helper and harness. The cleanup closure cannot run
/// until prepare, consume and agent drain have all succeeded through the
/// authenticated transport. Failed transactions attempt a bounded abort.
pub fn perform_removal<T, W, C>(
    delete_data: bool,
    mut transport: T,
    mut wait: W,
    mut cleanup: C,
) -> Result<(), IpcError>
where
    T: FnMut(MaintenanceRequest) -> Result<MaintenanceReply, IpcError>,
    W: FnMut() -> Result<(), IpcError>,
    C: FnMut(&str) -> Result<(), IpcError>,
{
    let prepared = transport(if delete_data {
        MaintenanceRequest::PrepareCompleteRemoval
    } else {
        MaintenanceRequest::PrepareRemoval { delete_data: false }
    })?;
    if prepared.phase != "prepared" {
        return Err(IpcError::new(
            "protocol",
            "Expected prepared removal permit",
        ));
    }
    let transaction = prepared.transaction;
    let result = (|| {
        let consumed = transport(if delete_data {
            MaintenanceRequest::ConsumeCompleteRemoval {
                permit: transaction.clone(),
            }
        } else {
            MaintenanceRequest::ConsumeRemoval {
                permit: transaction.clone(),
                delete_data: false,
            }
        })?;
        if consumed.transaction != transaction
            || consumed.installation != prepared.installation
            || consumed.phase != "draining"
        {
            return Err(IpcError::new(
                "permit_mismatch",
                "Authority did not confirm this removal transaction",
            ));
        }
        loop {
            let status = transport(MaintenanceRequest::RemovalReady {
                transaction: transaction.clone(),
            })?;
            if status.transaction != transaction || status.installation != prepared.installation {
                return Err(IpcError::new(
                    "permit_mismatch",
                    "Authority transaction changed",
                ));
            }
            if status.phase == "ready" {
                break;
            }
            if status.phase != "draining" {
                return Err(IpcError::new("protocol", "Unexpected removal phase"));
            }
            wait()?;
        }
        cleanup(&transaction)
    })();
    if result.is_err() {
        let _ = transport(MaintenanceRequest::AbortRemoval { transaction });
    }
    result
}

#[cfg(test)]
mod transaction_tests {
    use super::*;
    #[test]
    fn interactive_direct_and_silent_denials_and_unavailable_authority_have_zero_cleanup() {
        for _mode in ["interactive", "direct", "silent"] {
            for delete_data in [false, true] {
                for code in [
                    "removal_protected",
                    "service_unavailable",
                    "permit_expired",
                    "policy_changed",
                ] {
                    let mut effects = 0;
                    let result = perform_removal(
                        delete_data,
                        |_| Err(IpcError::new(code, "denied")),
                        || Ok(()),
                        |_| {
                            effects += 1;
                            Ok(())
                        },
                    );
                    assert_eq!(result.unwrap_err().code, code);
                    assert_eq!(effects, 0);
                }
            }
        }
    }
    #[test]
    fn complete_cleanup_never_falls_back_to_an_older_services_removal_protocol() {
        let mut calls = 0;
        let mut effects = 0;
        let result = perform_removal(
            true,
            |request| {
                calls += 1;
                assert!(matches!(
                    request,
                    MaintenanceRequest::PrepareCompleteRemoval
                ));
                Err(IpcError::new(
                    "malformed",
                    "Older service rejects this operation",
                ))
            },
            || Ok(()),
            |_| {
                effects += 1;
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err().code, "malformed");
        assert_eq!(calls, 1);
        assert_eq!(effects, 0);
    }
    #[test]
    fn cleanup_is_after_consumption_and_agent_acknowledgement_and_failure_is_not_success() {
        use std::{cell::RefCell, rc::Rc};
        let events = Rc::new(RefCell::new(Vec::new()));
        let observed = events.clone();
        let cleanup_events = events.clone();
        let result = perform_removal(
            true,
            move |request| {
                let (event, phase) = match request {
                    MaintenanceRequest::PrepareCompleteRemoval => ("prepare", "prepared"),
                    MaintenanceRequest::ConsumeCompleteRemoval { .. } => ("consume", "draining"),
                    MaintenanceRequest::RemovalReady { .. } => ("agent_ack", "ready"),
                    MaintenanceRequest::AbortRemoval { .. } => ("abort", "aborted"),
                    _ => panic!(),
                };
                observed.borrow_mut().push(event);
                Ok(MaintenanceReply {
                    installation: "i".into(),
                    transaction: "t".into(),
                    phase: phase.into(),
                })
            },
            || Ok(()),
            move |_| {
                cleanup_events.borrow_mut().push("cleanup");
                Err(IpcError::new("cleanup_failed", "injected"))
            },
        );
        assert_eq!(result.unwrap_err().code, "cleanup_failed");
        assert_eq!(
            *events.borrow(),
            ["prepare", "consume", "agent_ack", "cleanup", "abort"]
        );
    }
    #[test]
    fn a_lost_agent_aborts_without_cleanup() {
        let mut effects = 0;
        let result = perform_removal(
            false,
            |r| {
                Ok(MaintenanceReply {
                    installation: "i".into(),
                    transaction: "t".into(),
                    phase: if matches!(r, MaintenanceRequest::PrepareRemoval { .. }) {
                        "prepared"
                    } else {
                        "draining"
                    }
                    .into(),
                })
            },
            || Err(IpcError::new("agent_cleanup_timeout", "no acknowledgement")),
            |_| {
                effects += 1;
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err().code, "agent_cleanup_timeout");
        assert_eq!(effects, 0);
    }
}
