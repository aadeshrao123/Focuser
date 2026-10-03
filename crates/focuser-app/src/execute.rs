//! Command dispatch — the single place application behaviour lives.
//!
//! The `match` is exhaustive with no `_` arm on purpose: adding a [`Command`]
//! variant without handling it here is a compile error.

use focuser_common::allowance::Allowance;
use focuser_common::host::canonical_host;
use focuser_common::types::{
    AppRule, BlockList, EntityId, ExceptionRule, Lock, Protection, Schedule, WebsiteMatchType,
    WebsiteRule,
};
use focuser_core::{BlockEngine, pomodoro};

use crate::command::{
    AllowanceNotificationDto, AllowanceUsageEntry, AppIcon, BlockingHealth, BrowserStatus, Command,
    CommandResult, LockSetup, PomodoroEventDto, PomodoroHistoryEntry, ProtectionInfo,
};
use crate::context::{AppContext, PomodoroEvent};
use crate::error::{CommandError, CommandOutcome};

/// Run one command against the shared context.
pub fn execute(ctx: &AppContext, cmd: Command) -> CommandOutcome<CommandResult> {
    // Before the lock: reading icons is slow and never touches the engine.
    if let Command::GetAppIcons { targets } = cmd {
        return Ok(CommandResult::AppIcons(app_icons(targets)));
    }

    let mut engine = ctx
        .engine
        .lock()
        .map_err(|_| CommandError::Internal("engine lock poisoned".into()))?;

    match cmd {
        Command::ListBlockLists => Ok(CommandResult::BlockLists(engine.block_lists().to_vec())),

        Command::CreateBlockList { name } => {
            let name = name.trim();
            if name.is_empty() {
                return Err(CommandError::Validation("name must not be empty".into()));
            }

            let list = BlockList::new(name);
            engine.db().create_block_list(&list)?;
            engine.refresh()?;
            Ok(CommandResult::BlockList(Box::new(list)))
        }

        Command::UpdateBlockList { list } => {
            ensure_unprotected(&engine, list.id)?;

            let mut list = *list;
            // `protection` and `lock` can only be set by `EnableProtection`
            // and cleared by `UnlockProtection` (or by expiry) — never by a
            // wholesale replace. Without this, a caller could hand back the
            // list it just fetched with `protection: null` and walk straight
            // out of a commitment it made moments earlier.
            let stored = engine.db().get_block_list(list.id)?;
            list.protection = stored.protection;
            list.lock = stored.lock;
            list.scheduled_protection = stored.scheduled_protection;
            list.schedule_unlocked_until = stored.schedule_unlocked_until;
            list.shared_allowance = stored.shared_allowance;
            list.reconcile_schedule_bypass();

            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::DeleteBlockList { id } => {
            ensure_unprotected(&engine, id)?;
            engine.db().delete_block_list(id)?;
            // Best-effort: an orphaned challenge row (protection expired
            // without ever being unlocked) would otherwise linger forever.
            let _ = engine.db().take_unlock_challenge(id);
            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::ToggleBlockList { id, enabled } => {
            // Scheduled locks freeze both toggle directions. Preserve the
            // existing manual-lock behavior that permits re-enabling.
            if !enabled
                || engine.block_lists().iter().any(|l| {
                    l.id == id && l.scheduled_protection_at(chrono::Local::now()).is_some()
                })
            {
                ensure_unprotected(&engine, id)?;
            }

            let mut list = engine.db().get_block_list(id)?;
            list.enabled = enabled;
            list.updated_at = chrono::Utc::now();
            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        // ─── Website rules ────────────────────────────────────────
        Command::AddWebsiteRule { list_id, rule } => {
            let mut match_type = rule;
            normalize(&mut match_type);
            let created = WebsiteRule {
                id: focuser_common::types::new_id(),
                match_type,
                enabled: true,
            };
            let key = website_key(&created);
            let mut out = created.clone();

            mutate_list(ctx, &mut engine, list_id, |list| {
                // Adding the same site twice should not make two rules.
                match list.websites.iter().find(|r| website_key(r) == key) {
                    Some(existing) => out = existing.clone(),
                    None => list.websites.push(created),
                }
                Ok(())
            })?;
            Ok(CommandResult::WebsiteRule(Box::new(out)))
        }

        Command::RemoveWebsiteRule { list_id, rule_id } => {
            mutate_list(ctx, &mut engine, list_id, |list| {
                remove_by_id(&mut list.websites, rule_id, |r| r.id)
            })?;
            Ok(CommandResult::Unit)
        }

        Command::BulkImportWebsites {
            list_id,
            values,
            kind,
        } => {
            let mut added = 0u32;
            mutate_list(ctx, &mut engine, list_id, |list| {
                for raw in &values {
                    let value = raw.trim().to_lowercase();
                    // Blank lines and `#` comments come from pasted host files
                    // and block-list exports; they are not rules.
                    if value.is_empty() || value.starts_with('#') {
                        continue;
                    }

                    let mut candidate = kind.rule(&value);
                    normalize(&mut candidate.match_type);
                    if website_value(&candidate).is_some_and(|v| v.is_empty()) {
                        continue;
                    }

                    let key = website_key(&candidate);
                    if list.websites.iter().any(|r| website_key(r) == key) {
                        continue;
                    }
                    list.websites.push(candidate);
                    added += 1;
                }
                Ok(())
            })?;
            Ok(CommandResult::Count(added))
        }

        Command::ClearAllWebsites => {
            let cleared = clear_across_lists(&mut engine, |list| {
                let n = list.websites.len() as u32;
                list.websites.clear();
                n
            })?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Count(cleared))
        }

        // ─── Application rules ────────────────────────────────────
        Command::AddAppRule { list_id, rule } => {
            let created = AppRule {
                id: focuser_common::types::new_id(),
                match_type: rule,
                enabled: true,
            };
            let out = created.clone();
            mutate_list(ctx, &mut engine, list_id, |list| {
                list.applications.push(created);
                Ok(())
            })?;
            Ok(CommandResult::AppRule(Box::new(out)))
        }

        Command::RemoveAppRule { list_id, rule_id } => {
            mutate_list(ctx, &mut engine, list_id, |list| {
                remove_by_id(&mut list.applications, rule_id, |r| r.id)
            })?;
            Ok(CommandResult::Unit)
        }

        Command::ClearAllApps => {
            let cleared = clear_across_lists(&mut engine, |list| {
                let n = list.applications.len() as u32;
                list.applications.clear();
                n
            })?;
            // No hosts sync: application rules never reach the hosts file.
            Ok(CommandResult::Count(cleared))
        }

        // ─── Exceptions ───────────────────────────────────────────
        Command::AddException { list_id, exception } => {
            // An address with a path allows that page, anything else the
            // site (#21). A blank value allows nothing, so it is refused.
            let exception_type = exception.normalized().ok_or_else(|| {
                CommandError::Validation("enter a domain or a page address".into())
            })?;
            let created = ExceptionRule {
                id: focuser_common::types::new_id(),
                exception_type,
                enabled: true,
            };
            let out = created.clone();
            mutate_list(ctx, &mut engine, list_id, |list| {
                list.exceptions.push(created);
                Ok(())
            })?;
            Ok(CommandResult::Exception(Box::new(out)))
        }

        Command::RemoveException {
            list_id,
            exception_id,
        } => {
            mutate_list(ctx, &mut engine, list_id, |list| {
                remove_by_id(&mut list.exceptions, exception_id, |e| e.id)
            })?;
            Ok(CommandResult::Unit)
        }

        // ─── Schedule ─────────────────────────────────────────────
        Command::UpdateSchedule {
            list_id,
            slots,
            always_active,
        } => {
            mutate_list(ctx, &mut engine, list_id, |list| {
                list.schedule = if always_active {
                    // No schedule means "active whenever enabled".
                    None
                } else {
                    Some(Schedule {
                        id: focuser_common::types::new_id(),
                        name: format!("{} schedule", list.name),
                        time_slots: slots,
                        enabled: true,
                    })
                };
                if list.scheduled_protection.is_some() || list.shared_allowance.is_some() {
                    ensure_schedule_ends(list)?;
                }
                Ok(())
            })?;
            Ok(CommandResult::Unit)
        }

        // ─── Statistics ───────────────────────────────────────────
        Command::GetStats { from, to } => {
            validate_range(from, to)?;
            Ok(CommandResult::Stats(engine.db().get_stats(from, to)?))
        }

        Command::GetBlockedEvents { from, to } => {
            validate_range(from, to)?;
            let events = engine
                .db()
                .get_blocked_events(&from.to_string(), &to.to_string())?;
            Ok(CommandResult::BlockedEvents(events))
        }

        Command::ClearStatistics => {
            engine.db().clear_all_statistics()?;
            Ok(CommandResult::Unit)
        }

        Command::GetStatsRetention => {
            let days = engine
                .db()
                .get_setting_or_default(
                    SETTING_STATS_RETENTION,
                    &DEFAULT_STATS_RETENTION_DAYS.to_string(),
                )?
                .parse::<u32>()
                .unwrap_or(DEFAULT_STATS_RETENTION_DAYS);
            Ok(CommandResult::Count(days))
        }

        Command::SetStatsRetention { days } => {
            if !(1..=MAX_STATS_RETENTION_DAYS).contains(&days) {
                return Err(CommandError::Validation(format!(
                    "retention must be between 1 and {MAX_STATS_RETENTION_DAYS} days"
                )));
            }

            engine
                .db()
                .set_setting(SETTING_STATS_RETENTION, &days.to_string())?;
            let deleted = engine.db().cleanup_old_statistics(days)?;
            Ok(CommandResult::Count(deleted as u32))
        }

        // ─── Protection ───────────────────────────────────────────
        Command::EnableProtection {
            list_id,
            duration_minutes,
            prevent_uninstall,
            prevent_service_stop,
            prevent_modification,
            lock,
        } => {
            if duration_minutes == Some(0) {
                return Err(CommandError::Validation(
                    "protection duration must be at least 1 minute".into(),
                ));
            }
            if duration_minutes.is_none() && lock.is_none() {
                return Err(CommandError::Validation(
                    "a lock with no end needs an unlock method".into(),
                ));
            }

            let lock = prepare_lock(lock)?;

            let mut list = engine.db().get_block_list(list_id)?;
            if list.is_modification_protected() {
                // Re-arming would let a user extend or shorten a commitment they
                // already made, which defeats the point of protection.
                return Err(CommandError::Protected);
            }

            let now = chrono::Utc::now();
            list.protection = Some(Protection {
                prevent_uninstall,
                prevent_service_stop,
                prevent_modification,
                started_at: now,
                expires_at: duration_minutes
                    .map(|minutes| now + chrono::Duration::minutes(i64::from(minutes))),
            });
            list.lock = lock;
            // Protecting a disabled list would protect nothing.
            list.enabled = true;
            list.updated_at = now;

            // Any challenge left over from a previous window on this list is
            // for a lock that no longer exists.
            engine.db().take_unlock_challenge(list_id)?;

            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::ConfigureScheduledProtection {
            list_id,
            enabled,
            lock,
        } => {
            ensure_unprotected(&engine, list_id)?;
            let lock = prepare_lock(lock)?;
            let mut list = engine.db().get_block_list(list_id)?;
            if enabled {
                ensure_schedule_ends(&list)?;
            }
            list.scheduled_protection =
                enabled.then_some(focuser_common::types::ScheduledProtection { lock });
            list.updated_at = chrono::Utc::now();
            engine.db().take_unlock_challenge(list_id)?;
            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            Ok(CommandResult::Unit)
        }

        Command::ConfigureSharedAllowance { list_id, minutes } => {
            ensure_unprotected(&engine, list_id)?;
            let config =
                minutes.map(|minutes| focuser_common::allowance::SharedAllowanceConfig { minutes });
            if let Some(config) = &config {
                config.validate().map_err(CommandError::Validation)?;
            }
            let mut list = engine.db().get_block_list(list_id)?;
            if config.is_some() {
                ensure_schedule_ends(&list)?;
            }
            list.shared_allowance = config;
            list.updated_at = chrono::Utc::now();
            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            Ok(CommandResult::Unit)
        }
        Command::GetSharedAllowanceStatus => {
            let mut statuses = Vec::new();
            for list in engine.block_lists() {
                if let Some(status) = engine.db().shared_status_at(list, chrono::Local::now())? {
                    statuses.push(status);
                }
            }
            Ok(CommandResult::SharedAllowanceStatus(statuses))
        }
        Command::RelockScheduledProtection { list_id } => {
            let mut list = engine.db().get_block_list(list_id)?;
            list.schedule_unlocked_until = None;
            list.updated_at = chrono::Utc::now();
            engine.db().take_unlock_challenge(list_id)?;
            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            Ok(CommandResult::Unit)
        }
        Command::GetScheduledProtectionStatus => Ok(CommandResult::ScheduledProtectionStatus(
            engine
                .block_lists()
                .iter()
                .map(|list| crate::command::ScheduledProtectionStatus {
                    block_list_id: list.id,
                    state: list.scheduled_lock_state(),
                })
                .collect(),
        )),

        Command::GetProtectionStatus => {
            let infos = engine
                .block_lists()
                .iter()
                .filter(|l| l.has_active_protection())
                .filter_map(|l| {
                    let p = l.effective_protection()?;
                    Some(ProtectionInfo {
                        block_list_id: l.id,
                        block_list_name: l.name.clone(),
                        prevent_uninstall: p.prevent_uninstall,
                        prevent_service_stop: p.prevent_service_stop,
                        prevent_modification: p.prevent_modification,
                        remaining_seconds: p.remaining_seconds(),
                        expires_at: p.expires_at,
                    })
                })
                .collect();
            Ok(CommandResult::ProtectionStatus(infos))
        }

        Command::RequestUnlockChallenge { list_id } => {
            // Goes straight to the database rather than through `mutate_list`:
            // that helper's protection guard would refuse this exact call, since
            // it only ever runs while the list *is* protected.
            let list = engine.db().get_block_list(list_id)?;
            if !list.has_active_protection() {
                return Err(CommandError::Validation(
                    "this block list has no active protection to unlock".into(),
                ));
            }
            let fresh = match list.effective_lock() {
                Some(Lock::RandomText { length }) => Lock::random_text_of_length(*length),
                Some(Lock::Password { .. }) => {
                    return Err(CommandError::Validation(
                        "this list is locked with a password, not a typing challenge".into(),
                    ));
                }
                None => return Err(CommandError::Protected),
            };

            let challenge = engine.db().issue_unlock_challenge(list_id, &fresh)?;
            Ok(CommandResult::Text(challenge))
        }

        Command::UnlockProtection { list_id, response } => {
            let mut list = engine.db().get_block_list(list_id)?;
            if !list.has_active_protection() {
                return Err(CommandError::Validation(
                    "this block list has no active protection to unlock".into(),
                ));
            }

            // A password is checked exactly as typed, the way it was hashed.
            let verified = match list.effective_lock() {
                Some(lock @ Lock::Password { .. }) => lock.verify_password(&response),
                Some(Lock::RandomText { length }) => {
                    // Consumed unconditionally: right or wrong, this challenge
                    // is spent, so a wrong guess cannot be retried against it
                    // and a right one cannot be replayed. A text of another
                    // length was issued for the list's other lock.
                    engine
                        .db()
                        .take_unlock_challenge(list_id)?
                        .is_some_and(|issued| {
                            issued.len() == Lock::challenge_len(*length)
                                && issued == response.trim()
                        })
                }
                // No lock means no early unlock — the only way out is to wait.
                None => return Err(CommandError::Protected),
            };

            if !verified {
                return Err(CommandError::WrongUnlockResponse);
            }

            // End the manual commitment, or bypass this scheduled occurrence.
            // Neither action disables blocking. Independent overlapping manual
            // and scheduled commitments must each be unlocked.
            if list.protection.as_ref().is_some_and(|p| p.is_active()) {
                list.protection = None;
                list.lock = None;
            } else if let Some(p) = list.scheduled_protection_at(chrono::Local::now()) {
                list.schedule_unlocked_until = p.expires_at;
            }
            list.updated_at = chrono::Utc::now();

            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            Ok(CommandResult::Unit)
        }

        // ─── Settings ─────────────────────────────────────────────
        Command::GetSetting { key, default } => {
            let value = match engine.db().get_setting(&key)? {
                Some(v) => Some(v),
                None => default,
            };
            Ok(CommandResult::Setting(value))
        }

        Command::SetSetting { key, value } => {
            if key.trim().is_empty() {
                return Err(CommandError::Validation(
                    "setting key must not be empty".into(),
                ));
            }
            if loosens_enforcement(&engine, &key, &value)? {
                ensure_nothing_protected(&engine)?;
            }
            engine.db().set_setting(&key, &value)?;
            Ok(CommandResult::Unit)
        }

        Command::ResetSettings => {
            ensure_nothing_protected(&engine)?;
            engine.db().clear_settings()?;
            Ok(CommandResult::Unit)
        }

        // ─── Enforcement ──────────────────────────────────────────
        Command::GetBlockingHealth => {
            let active_lists = engine
                .block_lists()
                .iter()
                .filter(|l| l.is_effectively_active())
                .count() as u32;

            Ok(CommandResult::BlockingHealth(BlockingHealth {
                active_lists,
                extension_connected: !ctx.connected_browsers().is_empty(),
                hosts_writable: ctx.hosts_writable(),
                extension_only_rules: engine.has_extension_only_rules(),
                app_usage_measurable: focuser_common::session::app_usage_measurable(),
            }))
        }

        Command::ApplyBlocks => {
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::RemoveBlocks => {
            // "Unblock everything" is not on offer while a list is locked.
            ensure_nothing_protected(&engine)?;
            // An empty domain set is how "unblock everything" is expressed —
            // the sync writes the list, so an empty list clears the section.
            ctx.sync_hosts_with(&[]);
            Ok(CommandResult::Unit)
        }

        // ─── Pomodoro ─────────────────────────────────────────────
        Command::PomodoroPresets => Ok(CommandResult::PomodoroPresets(
            focuser_common::pomodoro::presets(),
        )),

        Command::PomodoroStatus => Ok(CommandResult::PomodoroStatus(pomodoro::build_status(
            engine.db(),
        )?)),

        Command::PomodoroStart {
            block_list_id,
            config,
        } => {
            config
                .validate()
                .map_err(|e| CommandError::Validation(e.to_string()))?;

            // No lock check: a session can only tighten a locked list. Its breaks
            // and its end leave such a list on (#14).
            let session = pomodoro::start_session(&mut engine, block_list_id, config)?;
            // A work phase suspends allowances and can change what is blocked.
            ctx.sync_hosts(&engine);
            Ok(CommandResult::PomodoroSession(Box::new(session)))
        }

        Command::PomodoroPause => Ok(CommandResult::Flag(pomodoro::pause_session(&mut engine)?)),

        Command::PomodoroResume => Ok(CommandResult::Flag(pomodoro::resume_session(&mut engine)?)),

        Command::PomodoroSkip => {
            let advanced = pomodoro::skip_phase(&mut engine)?.is_some();
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Flag(advanced))
        }

        Command::PomodoroStop => {
            let stopped = pomodoro::stop_session(&mut engine)?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Flag(stopped))
        }

        Command::PomodoroDrainEvents => {
            let events = ctx
                .drain_pomodoro_events()
                .into_iter()
                .map(|e| match e {
                    PomodoroEvent::PhaseAdvanced { to, cycle } => {
                        PomodoroEventDto::PhaseAdvanced { to, cycle }
                    }
                    PomodoroEvent::TamperDetected => PomodoroEventDto::TamperDetected,
                })
                .collect();
            Ok(CommandResult::PomodoroEvents(events))
        }

        Command::PomodoroHistory { days } => {
            let entries = engine
                .db()
                .get_pomodoro_history(days)?
                .into_iter()
                .map(
                    |(started_at, completed_cycles, total_work_secs)| PomodoroHistoryEntry {
                        started_at,
                        completed_cycles,
                        total_work_secs,
                    },
                )
                .collect();
            Ok(CommandResult::PomodoroHistory(entries))
        }

        // ─── Allowances ───────────────────────────────────────────
        Command::AllowanceList => Ok(CommandResult::Allowances(
            engine.db().allowance_statuses_with_shared()?,
        )),

        Command::AllowanceCreate {
            target,
            daily_limit_secs,
            strict_mode,
        } => {
            // An allowance with time left exempts its target from every list,
            // so adding, changing or resetting one could open a locked list.
            ensure_nothing_protected(&engine)?;
            let allowance = Allowance::new(target, daily_limit_secs, strict_mode);
            allowance.validate().map_err(CommandError::Validation)?;

            engine.db().create_allowance(&allowance)?;
            // The tracker caches which targets are exhausted; a new allowance
            // must be reflected immediately or it won't be enforced until restart.
            ctx.allowance_tracker.rebuild_from_db(engine.db())?;
            Ok(CommandResult::Allowance(Box::new(allowance)))
        }

        Command::AllowanceUpdate {
            id,
            daily_limit_secs,
            strict_mode,
            enabled,
        } => {
            ensure_nothing_protected(&engine)?;
            let mut allowance = engine
                .db()
                .get_allowance(id)?
                .ok_or(CommandError::AllowanceNotFound(id))?;

            allowance.daily_limit_secs = daily_limit_secs;
            allowance.strict_mode = strict_mode;
            allowance.enabled = enabled;
            allowance.validate().map_err(CommandError::Validation)?;

            engine.db().update_allowance(&allowance)?;
            ctx.allowance_tracker.rebuild_from_db(engine.db())?;
            Ok(CommandResult::Unit)
        }

        Command::AllowanceDelete { id } => {
            // Allowed under a lock: on a locked list this only takes an
            // exemption away.
            engine.db().delete_allowance(id)?;
            ctx.allowance_tracker.rebuild_from_db(engine.db())?;
            Ok(CommandResult::Unit)
        }

        Command::AllowanceResetToday { id } => {
            ensure_nothing_protected(&engine)?;
            engine.db().reset_allowance_usage_today(id)?;
            // Rebuild clears the exhausted flag, so the target unblocks at once.
            ctx.allowance_tracker.rebuild_from_db(engine.db())?;
            Ok(CommandResult::Unit)
        }

        Command::AllowanceDrainNotifications => {
            let notifications = ctx
                .allowance_tracker
                .take_notifications()
                .into_iter()
                .map(|n| AllowanceNotificationDto {
                    allowance_id: n.allowance_id,
                    target: n.target,
                    kind: format!("{:?}", n.kind).to_lowercase(),
                    used_secs: n.used_secs,
                    limit_secs: n.limit_secs,
                })
                .collect();
            Ok(CommandResult::AllowanceNotifications(notifications))
        }

        Command::AllowanceHistory { id, days } => {
            let entries = engine
                .db()
                .get_allowance_usage_history(id, days)?
                .into_iter()
                .map(|(date, used_secs)| AllowanceUsageEntry { date, used_secs })
                .collect();
            Ok(CommandResult::AllowanceHistory(entries))
        }

        // ─── Whole-configuration and diagnostics ──────────────
        Command::ExportConfiguration => {
            let document = ConfigDocument {
                version: CONFIG_FORMAT_VERSION,
                app: "Focuser".to_string(),
                exported_at: chrono::Utc::now(),
                block_lists: engine.block_lists().to_vec(),
            };
            let json = serde_json::to_string_pretty(&document)
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            Ok(CommandResult::Text(json))
        }

        Command::ImportConfiguration { json } => {
            let document: ConfigDocument = serde_json::from_str(&json).map_err(|e| {
                CommandError::Validation(format!("not a Focuser configuration file: {e}"))
            })?;
            if document.version > CONFIG_FORMAT_VERSION {
                return Err(CommandError::Validation(format!(
                    "file was written by a newer version of Focuser (format {} vs {CONFIG_FORMAT_VERSION})",
                    document.version
                )));
            }

            ensure_nothing_protected(&engine)?;

            for list in &document.block_lists {
                if let Some(config) = &list.shared_allowance {
                    config.validate().map_err(CommandError::Validation)?;
                }
            }
            for id in engine
                .block_lists()
                .iter()
                .map(|l| l.id)
                .collect::<Vec<_>>()
            {
                engine.db().delete_block_list(id)?;
                // Same reasoning as `DeleteBlockList`: an orphaned challenge
                // row for a list that no longer exists would otherwise
                // linger forever.
                let _ = engine.db().take_unlock_challenge(id);
            }
            for list in &document.block_lists {
                let mut list = list.clone();
                list.schedule_unlocked_until = None;
                engine.db().create_block_list(&list)?;
            }

            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Count(document.block_lists.len() as u32))
        }

        Command::DeleteAllData => {
            ensure_nothing_protected(&engine)?;
            engine.db().delete_all_data()?;
            engine.refresh()?;
            // Nothing is blocked any more, so clear the hosts file outright.
            ctx.sync_hosts_with(&[]);
            Ok(CommandResult::Unit)
        }

        Command::CheckDomain { domain } => {
            // A domain with allowance time left is reachable even though it
            // appears in a block list, so check that before the rules.
            let exemptions = ctx.allowance_exempt_domains(&engine);
            let allowed = focuser_common::host::any_host_matches(&exemptions, &domain)
                && !engine.scheduled_block_on_domain(&domain);

            Ok(CommandResult::Flag(
                !allowed && engine.check_domain(&domain).is_some(),
            ))
        }

        Command::GetBrowserStatus => {
            let running = ctx.running_browsers();
            let connected = ctx.connected_browsers();

            let statuses = focuser_common::browser::KNOWN_BROWSERS
                .iter()
                .map(|info| {
                    let browser = format!("{:?}", info.browser_type);
                    BrowserStatus {
                        running: running.contains(&browser),
                        extension_connected: connected.contains(&browser),
                        display_name: info.display_name.to_string(),
                        store_url: info.store_url().to_string(),
                        launch_name: info.launch_name().to_string(),
                        browser,
                    }
                })
                .collect();
            Ok(CommandResult::BrowserStatus(statuses))
        }

        Command::AppVersion => Ok(CommandResult::Text(env!("CARGO_PKG_VERSION").to_string())),

        // Answered above; kept correct rather than `unreachable!`.
        Command::GetAppIcons { targets } => Ok(CommandResult::AppIcons(app_icons(targets))),
    }
}

/// Deduplicated, and one loader for the batch — on Linux that parses every
/// installed desktop entry, which should happen once rather than once per rule.
fn app_icons(targets: Vec<String>) -> Vec<AppIcon> {
    let mut seen = std::collections::HashSet::new();
    let loader = focuser_common::appicon::Loader::new();

    targets
        .into_iter()
        .filter(|target| seen.insert(target.clone()))
        .map(|target| AppIcon {
            data_uri: loader.icon_for(&target),
            target,
        })
        .collect()
}

/// Bumped only when the exported shape changes incompatibly.
const CONFIG_FORMAT_VERSION: u32 = 1;

/// The exported/imported document. Typed rather than hand-built JSON so export
/// and import can never disagree about the shape.
#[derive(serde::Serialize, serde::Deserialize)]
struct ConfigDocument {
    version: u32,
    /// Only there so a human opening the file can tell what wrote it.
    app: String,
    exported_at: chrono::DateTime<chrono::Utc>,
    block_lists: Vec<BlockList>,
}

/// A scheduled lock and a shared allowance both last for one scheduled block,
/// so the list needs hours, and hours that stop. A list that is on all week,
/// with no hours set or with every hour set, has no block to tie them to: the
/// lock would never open and the allowance would never refill.
fn ensure_schedule_ends(list: &BlockList) -> CommandOutcome<()> {
    let on_all_week = list
        .schedule
        .as_ref()
        .is_none_or(|s| s.time_slots.is_empty() || s.never_ends());
    if on_all_week {
        return Err(CommandError::Validation(
            "this list is on all week, so a lock or a shared allowance that follows its \
             hours would never end; set hours with a gap on the Schedule page first"
                .into(),
        ));
    }
    Ok(())
}

/// Wholesale operations cannot replace or disable a protected list.
fn ensure_nothing_protected(engine: &BlockEngine) -> CommandOutcome<()> {
    for list in engine.block_lists() {
        if engine.is_block_list_protected(list.id) {
            return Err(CommandError::Protected);
        }
    }
    Ok(())
}

/// How the blocking loop treats browsers that have no extension connected.
pub const SETTING_CLOSE_BROWSERS: &str = "block_unsupported_browsers";
pub const SETTING_GRACE_PERIOD: &str = "extension_grace_period";
pub const DEFAULT_GRACE_PERIOD_SECS: u64 = 60;

/// Turning browser closing off, or giving browsers longer, is refused during a
/// lock (#18). Tightening either one is always fine.
fn loosens_enforcement(engine: &BlockEngine, key: &str, value: &str) -> CommandOutcome<bool> {
    Ok(match key {
        SETTING_CLOSE_BROWSERS => value != "true",
        SETTING_GRACE_PERIOD => {
            let current = engine
                .db()
                .get_setting(key)?
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_GRACE_PERIOD_SECS);
            value.parse::<u64>().map_or(true, |next| next > current)
        }
        _ => false,
    })
}

/// Statistics retention setting key and bounds.
const SETTING_STATS_RETENTION: &str = "stats_retention_days";
const DEFAULT_STATS_RETENTION_DAYS: u32 = 30;
/// ~100 years. Guards against a typo turning into an effectively unbounded table.
const MAX_STATS_RETENTION_DAYS: u32 = 36_500;

/// Reject inverted date ranges, which silently return nothing rather than erroring.
fn validate_range(from: chrono::NaiveDate, to: chrono::NaiveDate) -> CommandOutcome<()> {
    if from > to {
        Err(CommandError::Validation(format!(
            "start date {from} is after end date {to}"
        )))
    } else {
        Ok(())
    }
}

fn prepare_lock(lock: Option<LockSetup>) -> CommandOutcome<Option<Lock>> {
    Ok(match lock {
        None => None,
        Some(LockSetup::Password { password }) => {
            if password.trim().is_empty() {
                return Err(CommandError::Validation(
                    "password must not be empty".into(),
                ));
            }
            Some(Lock::password(&password)?)
        }
        Some(LockSetup::RandomText { length }) => {
            if !(Lock::MIN_RANDOM_TEXT_LEN..=Lock::MAX_RANDOM_TEXT_LEN).contains(&length) {
                return Err(CommandError::Validation(format!(
                    "random-text length must be between {} and {}",
                    Lock::MIN_RANDOM_TEXT_LEN,
                    Lock::MAX_RANDOM_TEXT_LEN
                )));
            }
            Some(Lock::RandomText { length })
        }
    })
}

/// Load a list, check protection, apply `edit`, persist, refresh, re-sync hosts.
///
/// Every rule command follows this shape; centralising it means the protection
/// check and the hosts re-sync cannot be forgotten on a new command.
fn mutate_list(
    ctx: &AppContext,
    engine: &mut BlockEngine,
    list_id: EntityId,
    edit: impl FnOnce(&mut BlockList) -> CommandOutcome<()>,
) -> CommandOutcome<()> {
    ensure_unprotected(engine, list_id)?;

    let mut list = engine.db().get_block_list(list_id)?;
    edit(&mut list)?;
    list.reconcile_schedule_bypass();
    list.updated_at = chrono::Utc::now();

    engine.db().update_block_list(&list)?;
    engine.refresh()?;
    ctx.sync_hosts(engine);
    Ok(())
}

/// Remove the element with `id`, or report it missing.
///
/// The old commands used `retain`, which silently succeeded when the id did not
/// exist — so a caller deleting a stale rule got "OK" and no indication that
/// nothing happened.
fn remove_by_id<T>(
    items: &mut Vec<T>,
    id: EntityId,
    id_of: impl Fn(&T) -> EntityId,
) -> CommandOutcome<()> {
    let before = items.len();
    items.retain(|item| id_of(item) != id);

    if items.len() == before {
        Err(CommandError::RuleNotFound(id))
    } else {
        Ok(())
    }
}

/// Apply `clear` to every list that isn't modification-protected.
///
/// Protected lists are skipped rather than erroring, because "clear everything"
/// is a bulk action: failing the whole operation because one list is locked
/// would be worse than clearing the rest and reporting the true count.
fn clear_across_lists(
    engine: &mut BlockEngine,
    mut clear: impl FnMut(&mut BlockList) -> u32,
) -> CommandOutcome<u32> {
    let mut cleared = 0u32;

    for mut list in engine.db().list_block_lists()? {
        if list.is_modification_protected() {
            continue;
        }
        let n = clear(&mut list);
        if n == 0 {
            continue;
        }
        cleared += n;
        list.updated_at = chrono::Utc::now();
        engine.db().update_block_list(&list)?;
    }

    engine.refresh()?;
    Ok(cleared)
}

/// The comparable value of a website rule, for duplicate detection.
/// `EntireInternet` has no value and can never duplicate a domain.
fn website_value(rule: &WebsiteRule) -> Option<&String> {
    match &rule.match_type {
        WebsiteMatchType::Domain(v)
        | WebsiteMatchType::Keyword(v)
        | WebsiteMatchType::Wildcard(v)
        | WebsiteMatchType::UrlPath(v) => Some(v),
        WebsiteMatchType::EntireInternet => None,
    }
}

/// What makes two website rules the same rule.
///
/// Domains compare canonically, so `www.pornhub.com`, `pornhub.com` and a
/// pasted URL are one entry rather than three. Patterns compare literally,
/// because `*.reddit.com` and `reddit.com` really are different rules.
fn website_key(rule: &WebsiteRule) -> (u8, String) {
    match &rule.match_type {
        WebsiteMatchType::Domain(v) => (0, canonical_host(v)),
        WebsiteMatchType::Keyword(v) => (1, v.trim().to_lowercase()),
        WebsiteMatchType::Wildcard(v) => (2, v.trim().to_lowercase()),
        WebsiteMatchType::UrlPath(v) => (3, v.trim().to_lowercase()),
        WebsiteMatchType::EntireInternet => (4, String::new()),
    }
}

/// A domain is stored in its canonical form; everything else as typed.
fn normalize(match_type: &mut WebsiteMatchType) {
    if let WebsiteMatchType::Domain(d) = match_type {
        *d = canonical_host(d);
    }
}

/// Reject mutations to a block list whose protection window is still open.
///
/// Centralised here on purpose. This check was previously duplicated inline in
/// four `service.rs` arms plus a separate `check_protected` in `commands.rs`,
/// and the CLI's `list disable` path skipped it entirely — so a protected list
/// could be disabled from the command line.
fn ensure_unprotected(engine: &BlockEngine, id: EntityId) -> CommandOutcome<()> {
    if engine.is_block_list_protected(id) {
        Err(CommandError::Protected)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::WebsiteRuleKind;
    use focuser_common::allowance::AllowanceMatch;
    use focuser_common::pomodoro::PomodoroConfig;
    use focuser_common::types::{AppMatchType, ExceptionType, TimeSlot};
    use focuser_core::Database;

    fn ctx() -> AppContext {
        let db = Database::open_in_memory().unwrap();
        AppContext::new_headless(BlockEngine::new(db).unwrap())
    }

    /// A context that behaves like a machine with the extension installed.
    /// Website allowances only count — and only exempt — when one is connected.
    fn ctx_with_extension() -> AppContext {
        struct Connected;
        impl crate::context::SystemSync for Connected {
            fn sync_hosts(&self, _domains: &[String]) {}
            fn connected_browsers(&self) -> Vec<String> {
                vec!["Chrome".to_string()]
            }
        }

        let db = Database::open_in_memory().unwrap();
        AppContext::new(
            BlockEngine::new(db).unwrap(),
            std::sync::Arc::new(Connected),
        )
    }

    /// A context that looks like an unelevated machine with no extension.
    fn ctx_without_hosts_access() -> AppContext {
        struct Unprivileged;
        impl crate::context::SystemSync for Unprivileged {
            fn sync_hosts(&self, _domains: &[String]) {}
            fn hosts_writable(&self) -> bool {
                false
            }
        }

        let db = Database::open_in_memory().unwrap();
        AppContext::new(
            BlockEngine::new(db).unwrap(),
            std::sync::Arc::new(Unprivileged),
        )
    }

    fn create(ctx: &AppContext, name: &str) -> BlockList {
        execute(
            ctx,
            Command::CreateBlockList {
                name: name.to_string(),
            },
        )
        .unwrap()
        .as_block_list()
        .unwrap()
        .clone()
    }

    #[test]
    fn create_then_list_returns_the_new_list() {
        let ctx = ctx();
        let created = create(&ctx, "Social media");

        let result = execute(&ctx, Command::ListBlockLists).unwrap();
        let lists = result.as_block_lists().unwrap();

        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].id, created.id);
        assert_eq!(lists[0].name, "Social media");
    }

    #[test]
    fn create_trims_whitespace_and_rejects_blank_names() {
        let ctx = ctx();

        let created = create(&ctx, "  Games  ");
        assert_eq!(created.name, "Games", "name should be trimmed");

        let err = execute(
            &ctx,
            Command::CreateBlockList {
                name: "   ".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "validation");
    }

    #[test]
    fn toggle_flips_enabled_and_persists() {
        let ctx = ctx();
        let list = create(&ctx, "Focus");
        assert!(list.enabled, "new lists start enabled");

        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: false,
            },
        )
        .unwrap();

        let result = execute(&ctx, Command::ListBlockLists).unwrap();
        assert!(!result.as_block_lists().unwrap()[0].enabled);
    }

    #[test]
    fn delete_removes_the_list() {
        let ctx = ctx();
        let list = create(&ctx, "Temp");

        execute(&ctx, Command::DeleteBlockList { id: list.id }).unwrap();

        let result = execute(&ctx, Command::ListBlockLists).unwrap();
        assert!(result.as_block_lists().unwrap().is_empty());
    }

    #[test]
    fn update_persists_a_renamed_list() {
        let ctx = ctx();
        let mut list = create(&ctx, "Before");
        list.name = "After".into();

        execute(
            &ctx,
            Command::UpdateBlockList {
                list: Box::new(list),
            },
        )
        .unwrap();

        let result = execute(&ctx, Command::ListBlockLists).unwrap();
        assert_eq!(result.as_block_lists().unwrap()[0].name, "After");
    }

    // ─── Rules and exceptions ─────────────────────────────────────

    fn lists(ctx: &AppContext) -> Vec<BlockList> {
        execute(ctx, Command::ListBlockLists)
            .unwrap()
            .as_block_lists()
            .unwrap()
            .to_vec()
    }

    #[test]
    fn add_website_rule_persists_with_its_match_type() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");

        execute(
            &ctx,
            Command::AddWebsiteRule {
                list_id: list.id,
                rule: WebsiteMatchType::Keyword("casino".into()),
            },
        )
        .unwrap();

        let stored = &lists(&ctx)[0].websites;
        assert_eq!(stored.len(), 1);
        assert!(matches!(
            &stored[0].match_type,
            WebsiteMatchType::Keyword(k) if k == "casino"
        ));
    }

    #[test]
    fn removing_a_missing_rule_reports_not_found() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");

        let err = execute(
            &ctx,
            Command::RemoveWebsiteRule {
                list_id: list.id,
                rule_id: EntityId::new_v4(),
            },
        )
        .unwrap_err();

        // The old `retain`-based command returned Ok here, so a caller deleting
        // an already-gone rule could not tell that nothing happened.
        assert_eq!(err.code(), "rule_not_found");
    }

    #[test]
    fn bulk_import_skips_blanks_comments_and_duplicates() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");

        let added = execute(
            &ctx,
            Command::BulkImportWebsites {
                list_id: list.id,
                values: vec![
                    "  Example.com ".into(), // trimmed + lowercased
                    "example.com".into(),    // duplicate of the above
                    "".into(),               // blank
                    "   ".into(),            // whitespace only
                    "# a comment".into(),    // comment
                    "other.com".into(),
                ],
                kind: WebsiteRuleKind::Domain,
            },
        )
        .unwrap()
        .as_count()
        .unwrap();

        assert_eq!(added, 2, "only example.com and other.com are real values");
        assert_eq!(lists(&ctx)[0].websites.len(), 2);
    }

    // A starter list holds bare domains, but people paste `www.` forms and whole
    // URLs. Comparing the raw strings made all of those separate entries.
    #[test]
    fn bulk_import_treats_every_form_of_a_domain_as_one_entry() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");

        let added = execute(
            &ctx,
            Command::BulkImportWebsites {
                list_id: list.id,
                values: vec![
                    "pornhub.com".into(),
                    "www.pornhub.com".into(),
                    "https://www.pornhub.com/".into(),
                    "PORNHUB.COM.".into(),
                ],
                kind: WebsiteRuleKind::Domain,
            },
        )
        .unwrap()
        .as_count()
        .unwrap();

        assert_eq!(added, 1, "all four are the same site");
        let websites = &lists(&ctx)[0].websites;
        assert_eq!(websites.len(), 1);
        assert!(matches!(
            &websites[0].match_type,
            WebsiteMatchType::Domain(d) if d == "pornhub.com"
        ));
    }

    #[test]
    fn adding_the_same_site_twice_does_not_make_two_rules() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");

        let add = |value: &str| {
            execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: list.id,
                    rule: WebsiteMatchType::Domain(value.into()),
                },
            )
            .unwrap()
        };

        let CommandResult::WebsiteRule(first) = add("reddit.com") else {
            panic!("expected a rule back");
        };
        let CommandResult::WebsiteRule(second) = add("https://www.reddit.com/r/rust") else {
            panic!("expected a rule back");
        };

        assert_eq!(first.id, second.id, "the existing rule comes back");
        assert_eq!(lists(&ctx)[0].websites.len(), 1);
    }

    #[test]
    fn a_wildcard_and_a_domain_of_the_same_name_are_different_rules() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");

        for rule in [
            WebsiteMatchType::Domain("reddit.com".into()),
            WebsiteMatchType::Wildcard("reddit.com".into()),
        ] {
            execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: list.id,
                    rule,
                },
            )
            .unwrap();
        }

        assert_eq!(lists(&ctx)[0].websites.len(), 2);
    }

    #[test]
    fn app_rules_add_and_remove() {
        let ctx = ctx();
        let list = create(&ctx, "Apps");

        let result = execute(
            &ctx,
            Command::AddAppRule {
                list_id: list.id,
                rule: AppMatchType::ExecutableName("discord.exe".into()),
            },
        )
        .unwrap();

        let CommandResult::AppRule(rule) = result else {
            panic!("expected an app rule back");
        };
        assert_eq!(lists(&ctx)[0].applications.len(), 1);

        execute(
            &ctx,
            Command::RemoveAppRule {
                list_id: list.id,
                rule_id: rule.id,
            },
        )
        .unwrap();
        assert!(lists(&ctx)[0].applications.is_empty());
    }

    #[test]
    fn exceptions_add_and_remove() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");

        let result = execute(
            &ctx,
            Command::AddException {
                list_id: list.id,
                exception: ExceptionType::Domain("docs.example.com".into()),
            },
        )
        .unwrap();

        let CommandResult::Exception(exc) = result else {
            panic!("expected an exception back");
        };

        execute(
            &ctx,
            Command::RemoveException {
                list_id: list.id,
                exception_id: exc.id,
            },
        )
        .unwrap();
        assert!(lists(&ctx)[0].exceptions.is_empty());
    }

    #[test]
    fn an_exception_is_stored_as_a_site_or_as_a_page() {
        let ctx = ctx();
        let list = create(&ctx, "Sites");
        let add = |typed: &str| {
            let added = execute(
                &ctx,
                Command::AddException {
                    list_id: list.id,
                    exception: ExceptionType::Domain(typed.into()),
                },
            );
            match added {
                Ok(CommandResult::Exception(exc)) => Some(exc.exception_type),
                _ => None,
            }
        };

        // #21: a pasted address lost its path and allowed the whole site.
        assert!(matches!(
            add("https://www.youtube.com/@YouTube"),
            Some(ExceptionType::UrlPath(page)) if page == "youtube.com/@YouTube"
        ));
        assert!(matches!(
            add("https://WWW.Reddit.com/"),
            Some(ExceptionType::Domain(host)) if host == "reddit.com"
        ));
        assert!(add("   ").is_none(), "nothing to allow");
    }

    #[test]
    fn clear_all_websites_counts_across_lists_and_leaves_apps_alone() {
        let ctx = ctx();
        for name in ["A", "B"] {
            let list = create(&ctx, name);
            execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: list.id,
                    rule: WebsiteMatchType::Domain(format!("{name}.com")),
                },
            )
            .unwrap();
            execute(
                &ctx,
                Command::AddAppRule {
                    list_id: list.id,
                    rule: AppMatchType::ExecutableName("game.exe".into()),
                },
            )
            .unwrap();
        }

        let cleared = execute(&ctx, Command::ClearAllWebsites)
            .unwrap()
            .as_count()
            .unwrap();

        assert_eq!(cleared, 2);
        for list in lists(&ctx) {
            assert!(list.websites.is_empty());
            assert_eq!(list.applications.len(), 1, "apps must be untouched");
        }
    }

    // ─── Schedule, stats, settings, protection ────────────────────

    #[test]
    fn schedule_round_trips_and_always_active_clears_it() {
        let ctx = ctx();
        let list = create(&ctx, "Work");
        let slot = TimeSlot::new(
            chrono::Weekday::Mon,
            chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        );

        execute(
            &ctx,
            Command::UpdateSchedule {
                list_id: list.id,
                slots: vec![slot],
                always_active: false,
            },
        )
        .unwrap();

        let schedule = lists(&ctx)[0].schedule.clone().expect("schedule stored");
        assert_eq!(schedule.time_slots.len(), 1);
        assert_eq!(schedule.time_slots[0].day, chrono::Weekday::Mon);

        execute(
            &ctx,
            Command::UpdateSchedule {
                list_id: list.id,
                slots: vec![],
                always_active: true,
            },
        )
        .unwrap();

        assert!(
            lists(&ctx)[0].schedule.is_none(),
            "always_active must clear the schedule, not store an empty one"
        );
    }

    #[test]
    fn stats_reject_an_inverted_date_range() {
        let ctx = ctx();
        let err = execute(
            &ctx,
            Command::GetStats {
                from: chrono::NaiveDate::from_ymd_opt(2026, 5, 10).unwrap(),
                to: chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            },
        )
        .unwrap_err();

        // Previously this returned an empty result set, which reads as
        // "no activity" rather than "you asked for a backwards range".
        assert_eq!(err.code(), "validation");
    }

    #[test]
    fn stats_retention_defaults_to_thirty_and_rejects_out_of_range() {
        let ctx = ctx();

        let days = execute(&ctx, Command::GetStatsRetention)
            .unwrap()
            .as_count()
            .unwrap();
        assert_eq!(days, 30);

        for bad in [0, 40_000] {
            let err = execute(&ctx, Command::SetStatsRetention { days: bad }).unwrap_err();
            assert_eq!(err.code(), "validation", "{bad} days should be rejected");
        }

        execute(&ctx, Command::SetStatsRetention { days: 7 }).unwrap();
        assert_eq!(
            execute(&ctx, Command::GetStatsRetention)
                .unwrap()
                .as_count()
                .unwrap(),
            7
        );
    }

    #[test]
    fn settings_round_trip_and_fall_back_to_the_supplied_default() {
        let ctx = ctx();

        let missing = execute(
            &ctx,
            Command::GetSetting {
                key: "theme".into(),
                default: Some("dark".into()),
            },
        )
        .unwrap();
        assert!(matches!(missing, CommandResult::Setting(Some(v)) if v == "dark"));

        execute(
            &ctx,
            Command::SetSetting {
                key: "theme".into(),
                value: "light".into(),
            },
        )
        .unwrap();

        let stored = execute(
            &ctx,
            Command::GetSetting {
                key: "theme".into(),
                default: Some("dark".into()),
            },
        )
        .unwrap();
        assert!(
            matches!(stored, CommandResult::Setting(Some(v)) if v == "light"),
            "a stored value must win over the default"
        );
    }

    fn protect(ctx: &AppContext, id: EntityId) -> CommandOutcome<CommandResult> {
        protect_with_lock(ctx, id, None)
    }

    fn protect_with_lock(
        ctx: &AppContext,
        id: EntityId,
        lock: Option<LockSetup>,
    ) -> CommandOutcome<CommandResult> {
        execute(
            ctx,
            Command::EnableProtection {
                list_id: id,
                duration_minutes: Some(60),
                prevent_uninstall: true,
                prevent_service_stop: true,
                prevent_modification: true,
                lock,
            },
        )
    }

    #[test]
    fn protection_blocks_modification_and_disabling_but_not_enabling() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect(&ctx, list.id).unwrap();

        // Disabling would escape the commitment.
        let err = execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: false,
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "protected");

        // So would deleting it, or editing its rules.
        assert_eq!(
            execute(&ctx, Command::DeleteBlockList { id: list.id })
                .unwrap_err()
                .code(),
            "protected"
        );
        assert_eq!(
            execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: list.id,
                    rule: WebsiteMatchType::Domain("x.com".into()),
                },
            )
            .unwrap_err()
            .code(),
            "protected"
        );

        // Re-enabling is harmless and must stay allowed.
        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: true,
            },
        )
        .unwrap();
    }

    #[test]
    fn protection_cannot_be_re_armed_while_active() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect(&ctx, list.id).unwrap();

        // Otherwise a user could shorten a commitment they already made.
        assert_eq!(protect(&ctx, list.id).unwrap_err().code(), "protected");
    }

    #[test]
    fn protection_status_reports_the_active_window() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect(&ctx, list.id).unwrap();

        let result = execute(&ctx, Command::GetProtectionStatus).unwrap();
        let CommandResult::ProtectionStatus(infos) = result else {
            panic!("expected protection status");
        };

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].block_list_id, list.id);
        assert!(infos[0].remaining_seconds.is_some_and(|s| s > 0));
    }

    fn scheduled_list(ctx: &AppContext, lock: Option<LockSetup>) -> BlockList {
        use chrono::Datelike;
        let mut list = create(ctx, "Scheduled");
        list.schedule = Some(Schedule {
            id: focuser_common::types::new_id(),
            name: "Today".into(),
            enabled: true,
            time_slots: vec![TimeSlot::new(
                chrono::Local::now().weekday(),
                chrono::NaiveTime::MIN,
                chrono::NaiveTime::MIN,
            )],
        });
        execute(
            ctx,
            Command::UpdateBlockList {
                list: Box::new(list.clone()),
            },
        )
        .unwrap();
        execute(
            ctx,
            Command::ConfigureScheduledProtection {
                list_id: list.id,
                enabled: true,
                lock,
            },
        )
        .unwrap();
        list
    }

    /// Hours on a day that is not today: enough for a lock or a shared allowance
    /// to be set up, without the list being in its hours while the test runs.
    fn hours_later_this_week(ctx: &AppContext, list_id: EntityId) {
        use chrono::Datelike;
        execute(
            ctx,
            Command::UpdateSchedule {
                list_id,
                slots: vec![TimeSlot::new(
                    chrono::Local::now().weekday().succ().succ(),
                    chrono::NaiveTime::MIN,
                    chrono::NaiveTime::MIN,
                )],
                always_active: false,
            },
        )
        .unwrap();
    }

    #[test]
    fn shared_allowance_configuration_is_guarded_but_consumption_never_unlocks() {
        let ctx = ctx();
        let list = scheduled_list(
            &ctx,
            Some(LockSetup::Password {
                password: "secret".into(),
            }),
        );
        for minutes in [Some(30), Some(15), None] {
            assert!(
                execute(
                    &ctx,
                    Command::ConfigureSharedAllowance {
                        list_id: list.id,
                        minutes
                    }
                )
                .is_err()
            );
        }
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "secret".into(),
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::ConfigureSharedAllowance {
                list_id: list.id,
                minutes: Some(1),
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::AddWebsiteRule {
                list_id: list.id,
                rule: WebsiteMatchType::Domain("youtube.com".into()),
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::RelockScheduledProtection { list_id: list.id },
        )
        .unwrap();
        {
            let engine = ctx.engine.lock().unwrap();
            let tick = focuser_common::allowance::AllowanceTick {
                hostname: Some("youtube.com".into()),
                url: Some("https://youtube.com/".into()),
                app_exe: None,
                active: true,
                shared_active: true,
                shared_only: true,
                source: "test".into(),
                increment_secs: Some(20),
            };
            engine
                .db()
                .ingest_shared_at(&tick, chrono::Local::now())
                .unwrap();
            assert!(engine.is_block_list_protected(list.id));
            assert!(
                engine
                    .db()
                    .get_block_list(list.id)
                    .unwrap()
                    .schedule_unlocked_until
                    .is_none()
            );
            assert_eq!(
                engine
                    .db()
                    .shared_status_at(
                        &engine.db().get_block_list(list.id).unwrap(),
                        chrono::Local::now()
                    )
                    .unwrap()
                    .unwrap()
                    .remaining_secs,
                40
            );
        }
        assert!(
            execute(
                &ctx,
                Command::ConfigureSharedAllowance {
                    list_id: list.id,
                    minutes: None
                }
            )
            .is_err()
        );
    }

    #[test]
    fn shared_allowance_import_discards_runtime_fields_and_editing_bypass() {
        let ctx = ctx();
        let list = scheduled_list(
            &ctx,
            Some(LockSetup::Password {
                password: "secret".into(),
            }),
        );
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "secret".into(),
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::ConfigureSharedAllowance {
                list_id: list.id,
                minutes: Some(30),
            },
        )
        .unwrap();
        let CommandResult::Text(json) = execute(&ctx, Command::ExportConfiguration).unwrap() else {
            panic!("export")
        };
        let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
        doc["block_lists"][0]["shared_allowance"]["used_secs"] = serde_json::json!(1800);
        doc["block_lists"][0]["schedule_unlocked_until"] =
            serde_json::json!("2099-01-01T00:00:00Z");
        execute(
            &ctx,
            Command::ImportConfiguration {
                json: doc.to_string(),
            },
        )
        .unwrap();
        let engine = ctx.engine.lock().unwrap();
        let stored = engine.db().get_block_list(list.id).unwrap();
        assert!(stored.schedule_unlocked_until.is_none());
        assert!(stored.is_modification_protected());
        assert_eq!(
            engine
                .db()
                .shared_status_at(&stored, chrono::Local::now())
                .unwrap()
                .unwrap()
                .remaining_secs,
            1800
        );
    }

    #[test]
    fn shared_allowance_update_cannot_import_runtime_or_bypass_validation() {
        let ctx = ctx();
        let mut list = create(&ctx, "Shared");
        assert!(
            execute(
                &ctx,
                Command::ConfigureSharedAllowance {
                    list_id: list.id,
                    minutes: Some(0)
                }
            )
            .is_err()
        );
        list.shared_allowance =
            Some(focuser_common::allowance::SharedAllowanceConfig { minutes: 100 });
        execute(
            &ctx,
            Command::UpdateBlockList {
                list: Box::new(list.clone()),
            },
        )
        .unwrap();
        hours_later_this_week(&ctx, list.id);
        assert!(
            ctx.engine
                .lock()
                .unwrap()
                .db()
                .get_block_list(list.id)
                .unwrap()
                .shared_allowance
                .is_none()
        );
        execute(
            &ctx,
            Command::ConfigureSharedAllowance {
                list_id: list.id,
                minutes: Some(30),
            },
        )
        .unwrap();
        assert_eq!(
            ctx.engine
                .lock()
                .unwrap()
                .db()
                .get_block_list(list.id)
                .unwrap()
                .shared_allowance
                .unwrap()
                .minutes,
            30
        );
    }

    #[test]
    fn scheduled_unlock_allows_repeated_edits_and_toggle_and_restart() {
        let ctx = ctx();
        let list = scheduled_list(&ctx, Some(LockSetup::RandomText { length: 16 }));
        assert_eq!(
            execute(
                &ctx,
                Command::ToggleBlockList {
                    id: list.id,
                    enabled: false
                }
            )
            .unwrap_err()
            .code(),
            "protected"
        );
        let CommandResult::Text(challenge) =
            execute(&ctx, Command::RequestUnlockChallenge { list_id: list.id }).unwrap()
        else {
            panic!()
        };
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: challenge,
            },
        )
        .unwrap();
        for enabled in [false, true, false, true] {
            execute(
                &ctx,
                Command::ToggleBlockList {
                    id: list.id,
                    enabled,
                },
            )
            .unwrap();
        }
        for _ in 0..2 {
            let CommandResult::WebsiteRule(site) = execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: list.id,
                    rule: WebsiteMatchType::Domain("example.com".into()),
                },
            )
            .unwrap() else {
                panic!("website rule expected")
            };
            execute(
                &ctx,
                Command::RemoveWebsiteRule {
                    list_id: list.id,
                    rule_id: site.id,
                },
            )
            .unwrap();
            let CommandResult::AppRule(app) = execute(
                &ctx,
                Command::AddAppRule {
                    list_id: list.id,
                    rule: AppMatchType::ExecutableName("example-app".into()),
                },
            )
            .unwrap() else {
                panic!("app rule expected")
            };
            execute(
                &ctx,
                Command::RemoveAppRule {
                    list_id: list.id,
                    rule_id: app.id,
                },
            )
            .unwrap();
            execute(
                &ctx,
                Command::UpdateSchedule {
                    list_id: list.id,
                    slots: list.schedule.as_ref().unwrap().time_slots.clone(),
                    always_active: false,
                },
            )
            .unwrap();
        }
        for name in ["First edit", "Second edit"] {
            let mut updated = ctx
                .engine
                .lock()
                .unwrap()
                .db()
                .get_block_list(list.id)
                .unwrap();
            updated.name = name.into();
            // A caller cannot overwrite the trusted bypass, even by accident.
            updated.schedule_unlocked_until = None;
            execute(
                &ctx,
                Command::UpdateBlockList {
                    list: Box::new(updated),
                },
            )
            .unwrap();
        }
        let restored = ctx
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap();
        assert!(!restored.is_modification_protected());
        assert!(restored.schedule_unlocked_until.is_some());
        let db = Database::open_in_memory().unwrap();
        db.create_block_list(&restored).unwrap();
        let restarted = AppContext::new_headless(BlockEngine::new(db).unwrap());
        execute(
            &restarted,
            Command::ToggleBlockList {
                id: list.id,
                enabled: false,
            },
        )
        .unwrap();
    }

    #[test]
    fn unprotected_schedules_remain_editable_and_disabled_lists_rearm() {
        let ctx = ctx();
        let list = scheduled_list(&ctx, None);
        // Simulate an inactive period without waiting for the wall clock.
        {
            let mut engine = ctx.engine.lock().unwrap();
            let mut stored = engine.db().get_block_list(list.id).unwrap();
            stored.schedule.as_mut().unwrap().enabled = false;
            engine.db().update_block_list(&stored).unwrap();
            engine.refresh().unwrap();
        }
        execute(
            &ctx,
            Command::ConfigureScheduledProtection {
                list_id: list.id,
                enabled: false,
                lock: None,
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::UpdateSchedule {
                list_id: list.id,
                slots: list.schedule.unwrap().time_slots,
                always_active: false,
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::AddWebsiteRule {
                list_id: list.id,
                rule: WebsiteMatchType::Domain("example.com".into()),
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: false,
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::ConfigureScheduledProtection {
                list_id: list.id,
                enabled: true,
                lock: None,
            },
        )
        .unwrap();
        assert!(!ctx.engine.lock().unwrap().is_block_list_protected(list.id));
        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: true,
            },
        )
        .unwrap();
        assert!(ctx.engine.lock().unwrap().is_block_list_protected(list.id));
    }

    #[test]
    fn scheduled_configuration_uses_existing_challenge_length_limits() {
        let ctx = ctx();
        let list = create(&ctx, "Limits");
        hours_later_this_week(&ctx, list.id);
        for length in [Lock::MIN_RANDOM_TEXT_LEN - 1, Lock::MAX_RANDOM_TEXT_LEN + 1] {
            assert!(
                execute(
                    &ctx,
                    Command::ConfigureScheduledProtection {
                        list_id: list.id,
                        enabled: true,
                        lock: Some(LockSetup::RandomText { length }),
                    }
                )
                .is_err()
            );
        }
        for length in [Lock::MIN_RANDOM_TEXT_LEN, Lock::MAX_RANDOM_TEXT_LEN] {
            execute(
                &ctx,
                Command::ConfigureScheduledProtection {
                    list_id: list.id,
                    enabled: true,
                    lock: Some(LockSetup::RandomText { length }),
                },
            )
            .unwrap();
        }
    }

    #[test]
    fn hours_with_no_gap_cannot_carry_a_lock_or_a_shared_allowance() {
        use chrono::{Datelike, NaiveTime, Weekday::*};
        let ctx = ctx();
        let list = create(&ctx, "Always");
        let hours = |days: &[chrono::Weekday]| Command::UpdateSchedule {
            list_id: list.id,
            slots: days
                .iter()
                .map(|day| TimeSlot::new(*day, NaiveTime::MIN, NaiveTime::MIN))
                .collect(),
            always_active: false,
        };
        let lock = || Command::ConfigureScheduledProtection {
            list_id: list.id,
            enabled: true,
            lock: None,
        };
        let share = || Command::ConfigureSharedAllowance {
            list_id: list.id,
            minutes: Some(30),
        };
        let every_day = [Mon, Tue, Wed, Thu, Fri, Sat, Sun];
        let refused = |cmd| assert_eq!(execute(&ctx, cmd).unwrap_err().code(), "validation");

        // No hours set is "on all week" as much as every hour set is. With no
        // way to unlock, a lock on either would refuse quitting, uninstalling
        // and editing for good.
        refused(lock());
        refused(share());
        execute(&ctx, hours(&every_day)).unwrap();
        refused(lock());
        refused(share());

        // A day that is not today, so turning the lock on does not lock the test out.
        let later = chrono::Local::now().weekday().succ().succ();
        execute(&ctx, hours(&[later])).unwrap();
        execute(&ctx, lock()).unwrap();
        execute(&ctx, share()).unwrap();
        // And a list that has them cannot be put back on all week.
        refused(hours(&every_day));
        refused(Command::UpdateSchedule {
            list_id: list.id,
            slots: vec![],
            always_active: true,
        });
        assert!(!ctx.engine.lock().unwrap().is_block_list_protected(list.id));
    }

    #[test]
    fn manual_and_scheduled_commitments_unlock_independently() {
        let ctx = ctx();
        let list = scheduled_list(
            &ctx,
            Some(LockSetup::Password {
                password: "scheduled".into(),
            }),
        );
        // A manual lock can predate the schedule becoming active.
        {
            let mut engine = ctx.engine.lock().unwrap();
            let mut stored = engine.db().get_block_list(list.id).unwrap();
            stored.protection = Some(Protection::for_duration(60));
            stored.lock = Some(Lock::password("manual").unwrap());
            engine.db().update_block_list(&stored).unwrap();
            engine.refresh().unwrap();
        }
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "manual".into(),
            },
        )
        .unwrap();
        assert!(ctx.engine.lock().unwrap().is_block_list_protected(list.id));
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "scheduled".into(),
            },
        )
        .unwrap();
        assert!(!ctx.engine.lock().unwrap().is_block_list_protected(list.id));
    }

    #[test]
    fn a_challenge_issued_for_one_lock_does_not_open_the_other() {
        let ctx = ctx();
        let list = scheduled_list(&ctx, Some(LockSetup::RandomText { length: 64 }));
        let challenge = || {
            let CommandResult::Text(text) =
                execute(&ctx, Command::RequestUnlockChallenge { list_id: list.id }).unwrap()
            else {
                panic!("challenge expected")
            };
            text
        };
        // A manual lock with a short text on top of the scheduled one. Its text
        // is asked for and never typed, and then the manual lock runs out.
        let leave_a_short_text_behind = || {
            let manual = |expires_in| {
                let mut engine = ctx.engine.lock().unwrap();
                let mut stored = engine.db().get_block_list(list.id).unwrap();
                let mut protection = Protection::for_duration(60);
                protection.expires_at =
                    Some(chrono::Utc::now() + chrono::Duration::minutes(expires_in));
                stored.protection = Some(protection);
                stored.lock = Some(Lock::RandomText { length: 6 });
                engine.db().update_block_list(&stored).unwrap();
                engine.refresh().unwrap();
            };
            manual(60);
            let short = challenge();
            assert_eq!(short.len(), 6);
            manual(-1);
            short
        };
        let unlock = |response: String| {
            execute(
                &ctx,
                Command::UnlockProtection {
                    list_id: list.id,
                    response,
                },
            )
        };
        let locked = || ctx.engine.lock().unwrap().is_block_list_protected(list.id);

        // Typed straight in, as the CLI can: six characters instead of 64.
        let short = leave_a_short_text_behind();
        assert!(matches!(
            unlock(short),
            Err(CommandError::WrongUnlockResponse)
        ));
        assert!(locked());

        // Asked for again, as the app does: the text is for the lock in force.
        leave_a_short_text_behind();
        let long = challenge();
        assert_eq!(long.len(), 64);
        assert_eq!(challenge(), long, "asking twice must not change the text");
        unlock(long).unwrap();
        assert!(!locked());
    }

    #[test]
    fn scheduled_lock_rejects_every_configuration_mutation_and_relock_restores_guards() {
        use focuser_common::types::ScheduledLockState;
        let ctx = ctx();
        let list = scheduled_list(
            &ctx,
            Some(LockSetup::Password {
                password: "secret".into(),
            }),
        );
        let site = WebsiteRule::domain("example.com");
        let app = AppRule::executable("game");
        let exception = ExceptionRule {
            id: focuser_common::types::new_id(),
            exception_type: ExceptionType::Domain("safe.example.com".into()),
            enabled: true,
        };
        // Seed rules before exercising removal, so a not-found error cannot pass this test.
        let mut stored = ctx
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap();
        stored.websites.push(site.clone());
        stored.applications.push(app.clone());
        stored.exceptions.push(exception.clone());
        {
            let mut engine = ctx.engine.lock().unwrap();
            engine.db().update_block_list(&stored).unwrap();
            engine.refresh().unwrap();
        }
        let commands = || {
            vec![
            Command::AddWebsiteRule { list_id: list.id, rule: WebsiteMatchType::Domain("another.com".into()) },
            Command::RemoveWebsiteRule { list_id: list.id, rule_id: site.id },
            Command::BulkImportWebsites { list_id: list.id, values: vec!["other.com".into()], kind: WebsiteRuleKind::Domain },
            Command::AddAppRule { list_id: list.id, rule: AppMatchType::ExecutableName("other-game".into()) },
            Command::RemoveAppRule { list_id: list.id, rule_id: app.id },
            Command::AddException { list_id: list.id, exception: ExceptionType::Domain("example.com".into()) },
            Command::RemoveException { list_id: list.id, exception_id: exception.id },
            Command::UpdateSchedule { list_id: list.id, slots: vec![], always_active: false },
            Command::UpdateSchedule { list_id: list.id, slots: vec![], always_active: true },
            Command::UpdateBlockList { list: Box::new(stored.clone()) },
            Command::ToggleBlockList { id: list.id, enabled: false },
            Command::ToggleBlockList { id: list.id, enabled: true },
            Command::DeleteBlockList { id: list.id },
            Command::ConfigureScheduledProtection { list_id: list.id, enabled: false, lock: None },
            Command::ConfigureScheduledProtection { list_id: list.id, enabled: true, lock: Some(LockSetup::RandomText { length: 64 }) },
            Command::ConfigureScheduledProtection { list_id: list.id, enabled: true, lock: Some(LockSetup::Password { password: "changed".into() }) },
            Command::RemoveBlocks,
            Command::AllowanceCreate { target: AllowanceMatch::Domain("example.com".into()), daily_limit_secs: 600, strict_mode: false },
            Command::AllowanceUpdate { id: list.id, daily_limit_secs: 600, strict_mode: false, enabled: false },
            Command::AllowanceResetToday { id: list.id },
            Command::DeleteAllData, Command::ResetSettings,
            Command::SetSetting { key: SETTING_CLOSE_BROWSERS.into(), value: "false".into() },
            Command::ImportConfiguration { json: r#"{"version":1,"app":"Focuser","exported_at":"2026-07-27T00:00:00Z","block_lists":[]}"#.into() },
        ]
        };
        for cmd in commands() {
            assert_eq!(execute(&ctx, cmd).unwrap_err().code(), "protected");
        }
        execute(&ctx, Command::ClearAllWebsites).unwrap();
        execute(&ctx, Command::ClearAllApps).unwrap();
        assert_eq!(
            ctx.engine
                .lock()
                .unwrap()
                .db()
                .get_block_list(list.id)
                .unwrap()
                .websites
                .len(),
            1
        );
        assert_eq!(
            ctx.engine
                .lock()
                .unwrap()
                .db()
                .get_block_list(list.id)
                .unwrap()
                .applications
                .len(),
            1
        );
        let state = || {
            ctx.engine
                .lock()
                .unwrap()
                .db()
                .get_block_list(list.id)
                .unwrap()
                .scheduled_lock_state()
        };
        assert_eq!(state(), ScheduledLockState::Locked);
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "secret".into(),
            },
        )
        .unwrap();
        assert_eq!(state(), ScheduledLockState::UnlockedForEditing);
        for _ in 0..2 {
            execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: list.id,
                    rule: WebsiteMatchType::Domain("editing.com".into()),
                },
            )
            .unwrap();
            execute(
                &ctx,
                Command::AddAppRule {
                    list_id: list.id,
                    rule: AppMatchType::ExecutableName("editing".into()),
                },
            )
            .unwrap();
        }
        let before = ctx
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap();
        execute(
            &ctx,
            Command::RelockScheduledProtection { list_id: list.id },
        )
        .unwrap();
        let after = ctx
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap();
        assert!(after.schedule_unlocked_until.is_none());
        assert_eq!(
            serde_json::to_value(before.schedule).unwrap(),
            serde_json::to_value(after.schedule).unwrap()
        );
        assert_eq!(
            serde_json::to_value(before.scheduled_protection).unwrap(),
            serde_json::to_value(after.scheduled_protection).unwrap()
        );
        assert_eq!(state(), ScheduledLockState::Locked);
        for cmd in commands() {
            assert_eq!(execute(&ctx, cmd).unwrap_err().code(), "protected");
        }
        execute(&ctx, Command::ListBlockLists).unwrap();
        execute(&ctx, Command::GetScheduledProtectionStatus).unwrap();
    }

    #[test]
    fn scheduled_password_and_none_reuse_unlock_guards() {
        for lock in [
            None,
            Some(LockSetup::Password {
                password: "secret".into(),
            }),
        ] {
            let ctx = ctx();
            let has_password = lock.is_some();
            let list = scheduled_list(&ctx, lock);
            assert!(
                execute(
                    &ctx,
                    Command::UnlockProtection {
                        list_id: list.id,
                        response: "wrong".into()
                    }
                )
                .is_err()
            );
            let result = execute(
                &ctx,
                Command::UnlockProtection {
                    list_id: list.id,
                    response: "secret".into(),
                },
            );
            assert_eq!(result.is_ok(), has_password);
        }
    }

    #[test]
    fn editing_schedule_after_unlock_clears_bypass_when_inactive() {
        let ctx = ctx();
        let list = scheduled_list(
            &ctx,
            Some(LockSetup::Password {
                password: "secret".into(),
            }),
        );
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "secret".into(),
            },
        )
        .unwrap();
        let mut updated = ctx
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap();
        updated.schedule.as_mut().unwrap().name = "Still active".into();
        execute(
            &ctx,
            Command::UpdateBlockList {
                list: Box::new(updated),
            },
        )
        .unwrap();
        let mut updated = ctx
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap();
        assert!(!updated.is_modification_protected());
        updated.schedule.as_mut().unwrap().enabled = false;
        execute(
            &ctx,
            Command::UpdateBlockList {
                list: Box::new(updated),
            },
        )
        .unwrap();
        let mut updated = ctx
            .engine
            .lock()
            .unwrap()
            .db()
            .get_block_list(list.id)
            .unwrap();
        assert!(updated.schedule_unlocked_until.is_none());
        updated.schedule.as_mut().unwrap().enabled = true;
        execute(
            &ctx,
            Command::UpdateBlockList {
                list: Box::new(updated),
            },
        )
        .unwrap();
        assert!(ctx.engine.lock().unwrap().is_block_list_protected(list.id));
    }

    // ─── Locks: password and random-text early unlock ──────────────

    #[test]
    fn a_wrong_password_leaves_protection_active() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(
            &ctx,
            list.id,
            Some(LockSetup::Password {
                password: "correct-horse".into(),
            }),
        )
        .unwrap();

        let err = execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "wrong-guess".into(),
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "wrong_unlock_response");

        // Still locked: disabling is still refused.
        assert_eq!(
            execute(
                &ctx,
                Command::ToggleBlockList {
                    id: list.id,
                    enabled: false,
                },
            )
            .unwrap_err()
            .code(),
            "protected"
        );
    }

    #[test]
    fn the_right_password_ends_protection_and_then_disabling_works() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(
            &ctx,
            list.id,
            Some(LockSetup::Password {
                password: "correct-horse".into(),
            }),
        )
        .unwrap();

        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "correct-horse".into(),
            },
        )
        .unwrap();

        // Unlocking ends the commitment but does not itself turn the list
        // off — that is a separate, explicit action.
        assert!(lists(&ctx)[0].enabled, "unlock must not disable the list");

        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: false,
            },
        )
        .unwrap();
        assert!(!lists(&ctx)[0].enabled);
    }

    fn protect_until_unlocked(
        ctx: &AppContext,
        id: EntityId,
        lock: Option<LockSetup>,
    ) -> CommandOutcome<CommandResult> {
        execute(
            ctx,
            Command::EnableProtection {
                list_id: id,
                duration_minutes: None,
                prevent_uninstall: true,
                prevent_service_stop: true,
                prevent_modification: true,
                lock,
            },
        )
    }

    #[test]
    fn a_lock_with_no_end_needs_an_unlock_method() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");

        let err = protect_until_unlocked(&ctx, list.id, None).unwrap_err();
        assert_eq!(err.code(), "validation");
        assert!(lists(&ctx)[0].protection.is_none());
    }

    #[test]
    fn a_lock_with_no_end_holds_until_the_password_is_given() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_until_unlocked(
            &ctx,
            list.id,
            Some(LockSetup::Password {
                password: "correct-horse".into(),
            }),
        )
        .unwrap();

        let CommandResult::ProtectionStatus(infos) =
            execute(&ctx, Command::GetProtectionStatus).unwrap()
        else {
            panic!("expected protection status");
        };
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].remaining_seconds, None);
        assert_eq!(infos[0].expires_at, None);
        assert_eq!(
            execute(
                &ctx,
                Command::ToggleBlockList {
                    id: list.id,
                    enabled: false,
                },
            )
            .unwrap_err()
            .code(),
            "protected"
        );
        {
            let engine = ctx.engine.lock().unwrap();
            assert!(engine.has_service_protection());
            assert!(engine.has_uninstall_protection());
        }

        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "correct-horse".into(),
            },
        )
        .unwrap();
        assert!(lists(&ctx)[0].protection.is_none());
    }

    #[test]
    fn random_text_can_be_up_to_5000_characters() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_until_unlocked(&ctx, list.id, Some(LockSetup::RandomText { length: 5000 }))
            .unwrap();

        let CommandResult::Text(challenge) =
            execute(&ctx, Command::RequestUnlockChallenge { list_id: list.id }).unwrap()
        else {
            panic!("expected a challenge");
        };
        assert_eq!(challenge.chars().count(), 5000);
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: challenge,
            },
        )
        .unwrap();
        assert!(lists(&ctx)[0].protection.is_none());
    }

    #[test]
    fn a_password_is_checked_exactly_as_it_was_set() {
        let ctx = ctx();
        let unlock = |list_id, response: &str| {
            execute(
                &ctx,
                Command::UnlockProtection {
                    list_id,
                    response: response.into(),
                },
            )
        };
        let list = create(&ctx, "Committed");
        let password = Some(LockSetup::Password {
            password: " secret ".into(),
        });
        protect_with_lock(&ctx, list.id, password).unwrap();

        // Trimming on one side only made this lock impossible to open.
        assert!(unlock(list.id, "secret").is_err());
        unlock(list.id, " secret ").unwrap();

        let blank = Some(LockSetup::Password {
            password: "   ".into(),
        });
        assert!(protect_with_lock(&ctx, list.id, blank).is_err());
    }

    #[test]
    fn a_consumed_random_text_challenge_cannot_be_replayed() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(&ctx, list.id, Some(LockSetup::RandomText { length: 12 })).unwrap();

        let CommandResult::Text(challenge) =
            execute(&ctx, Command::RequestUnlockChallenge { list_id: list.id }).unwrap()
        else {
            panic!("expected a challenge string");
        };
        assert_eq!(challenge.chars().count(), 12);

        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: challenge.clone(),
            },
        )
        .unwrap();

        // Re-protect with a fresh random-text lock and prove the *old*
        // challenge no longer answers it.
        protect_with_lock(&ctx, list.id, Some(LockSetup::RandomText { length: 12 })).unwrap();
        let err = execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: challenge,
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "wrong_unlock_response");
    }

    #[test]
    fn asking_for_the_challenge_twice_shows_the_one_that_unlocks() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(&ctx, list.id, Some(LockSetup::RandomText { length: 12 })).unwrap();
        let ask = || execute(&ctx, Command::RequestUnlockChallenge { list_id: list.id });

        let (Ok(CommandResult::Text(first)), Ok(CommandResult::Text(second))) = (ask(), ask())
        else {
            panic!("expected two challenge strings");
        };
        assert_eq!(first, second);

        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: first,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_wrong_random_text_answer_consumes_the_challenge_so_it_cannot_be_retried() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(&ctx, list.id, Some(LockSetup::RandomText { length: 12 })).unwrap();

        let CommandResult::Text(challenge) =
            execute(&ctx, Command::RequestUnlockChallenge { list_id: list.id }).unwrap()
        else {
            panic!("expected a challenge string");
        };

        assert_eq!(
            execute(
                &ctx,
                Command::UnlockProtection {
                    list_id: list.id,
                    response: "totally-wrong".into(),
                },
            )
            .unwrap_err()
            .code(),
            "wrong_unlock_response"
        );

        // The right string, if retried against the same (now spent)
        // challenge, must also fail — a wrong guess does not get an
        // unlimited number of attempts against one string.
        assert_eq!(
            execute(
                &ctx,
                Command::UnlockProtection {
                    list_id: list.id,
                    response: challenge,
                },
            )
            .unwrap_err()
            .code(),
            "wrong_unlock_response"
        );
    }

    #[test]
    fn unlocking_with_no_lock_configured_is_refused_as_protected() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect(&ctx, list.id).unwrap(); // no lock — must simply wait it out

        assert_eq!(
            execute(
                &ctx,
                Command::UnlockProtection {
                    list_id: list.id,
                    response: "anything".into(),
                },
            )
            .unwrap_err()
            .code(),
            "protected"
        );
    }

    #[test]
    fn requesting_a_challenge_on_a_password_lock_is_a_validation_error() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(
            &ctx,
            list.id,
            Some(LockSetup::Password {
                password: "x".into(),
            }),
        )
        .unwrap();

        assert_eq!(
            execute(&ctx, Command::RequestUnlockChallenge { list_id: list.id })
                .unwrap_err()
                .code(),
            "validation"
        );
    }

    #[test]
    fn unlocking_an_unprotected_list_is_a_validation_error_not_wrong_response() {
        let ctx = ctx();
        let list = create(&ctx, "Open");

        assert_eq!(
            execute(
                &ctx,
                Command::UnlockProtection {
                    list_id: list.id,
                    response: "anything".into(),
                },
            )
            .unwrap_err()
            .code(),
            "validation"
        );
    }

    #[test]
    fn update_block_list_cannot_strip_protection_or_its_lock() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(
            &ctx,
            list.id,
            Some(LockSetup::Password {
                password: "correct-horse".into(),
            }),
        )
        .unwrap();

        // `prevent_modification` defaults to true in `protect_with_lock`, so
        // this is already refused — but even if it were not, `protection`
        // and `lock` must never travel through a wholesale update. Prove the
        // stronger property directly against the stored data.
        let mut tampered = lists(&ctx)[0].clone();
        tampered.protection = None;
        tampered.lock = None;

        let err = execute(
            &ctx,
            Command::UpdateBlockList {
                list: Box::new(tampered),
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "protected");

        // Still protected and still locked with the same password.
        assert!(lists(&ctx)[0].has_active_protection());
        execute(
            &ctx,
            Command::UnlockProtection {
                list_id: list.id,
                response: "correct-horse".into(),
            },
        )
        .unwrap();
    }

    // `prevent_modification: false` means rule/name/schedule edits are
    // allowed while protected — but `protection` and `lock` themselves are
    // not "modification" in that sense; they are the commitment device, and
    // only `EnableProtection`/`UnlockProtection` may touch them. Without the
    // guard in `UpdateBlockList`, this exact call would silently erase an
    // uninstall/service-stop commitment the user just made.
    #[test]
    fn update_block_list_preserves_protection_even_when_modification_is_allowed() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        execute(
            &ctx,
            Command::EnableProtection {
                list_id: list.id,
                duration_minutes: Some(60),
                prevent_uninstall: true,
                prevent_service_stop: true,
                prevent_modification: false,
                lock: Some(LockSetup::Password {
                    password: "correct-horse".into(),
                }),
            },
        )
        .unwrap();

        let mut edited = lists(&ctx)[0].clone();
        edited.name = "Renamed".into();
        edited.protection = None;
        edited.lock = None;

        execute(
            &ctx,
            Command::UpdateBlockList {
                list: Box::new(edited),
            },
        )
        .unwrap();

        let stored = lists(&ctx)[0].clone();
        assert_eq!(stored.name, "Renamed", "the actual edit must go through");
        assert!(
            stored.has_active_protection(),
            "protection must survive an update that only touched the name"
        );
        assert!(stored.lock.is_some(), "the lock must survive it too");
    }

    #[test]
    fn a_password_lock_never_persists_the_plaintext() {
        let ctx = ctx();
        let list = create(&ctx, "Committed");
        protect_with_lock(
            &ctx,
            list.id,
            Some(LockSetup::Password {
                password: "super-secret-phrase".into(),
            }),
        )
        .unwrap();

        let stored = lists(&ctx)[0].clone();
        let json = serde_json::to_string(&stored).unwrap();
        assert!(!json.contains("super-secret-phrase"));
    }

    #[test]
    fn clear_all_skips_protected_lists_but_still_clears_the_rest() {
        let ctx = ctx();

        let open = create(&ctx, "Open");
        let locked = create(&ctx, "Locked");
        for id in [open.id, locked.id] {
            execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: id,
                    rule: WebsiteMatchType::Domain("x.com".into()),
                },
            )
            .unwrap();
        }
        protect(&ctx, locked.id).unwrap();

        let cleared = execute(&ctx, Command::ClearAllWebsites)
            .unwrap()
            .as_count()
            .unwrap();

        // Bulk clear should not fail wholesale because one list is locked.
        assert_eq!(cleared, 1);
        let all = lists(&ctx);
        let locked_now = all.iter().find(|l| l.id == locked.id).unwrap();
        let open_now = all.iter().find(|l| l.id == open.id).unwrap();
        assert_eq!(locked_now.websites.len(), 1, "protected list untouched");
        assert!(open_now.websites.is_empty());
    }

    // ─── Pomodoro and allowances ──────────────────────────────────

    #[test]
    fn pomodoro_is_idle_until_started_and_reports_its_session() {
        let ctx = ctx();
        let list = create(&ctx, "Focus");

        let idle = execute(&ctx, Command::PomodoroStatus).unwrap();
        assert!(matches!(idle, CommandResult::PomodoroStatus(None)));

        execute(
            &ctx,
            Command::PomodoroStart {
                block_list_id: list.id,
                config: PomodoroConfig {
                    work_secs: 1500,
                    short_break_secs: 300,
                    long_break_secs: 900,
                    cycles_until_long_break: 4,
                },
            },
        )
        .unwrap();

        let running = execute(&ctx, Command::PomodoroStatus).unwrap();
        assert!(matches!(running, CommandResult::PomodoroStatus(Some(_))));
    }

    #[test]
    fn pomodoro_start_rejects_an_invalid_config() {
        let ctx = ctx();
        let list = create(&ctx, "Focus");

        let err = execute(
            &ctx,
            Command::PomodoroStart {
                block_list_id: list.id,
                config: PomodoroConfig {
                    work_secs: 0, // a zero-length work phase is meaningless
                    short_break_secs: 300,
                    long_break_secs: 900,
                    cycles_until_long_break: 4,
                },
            },
        )
        .unwrap_err();

        assert_eq!(err.code(), "validation");
    }

    #[test]
    fn pomodoro_pause_and_stop_report_whether_anything_happened() {
        let ctx = ctx();

        // Nothing running: these are no-ops, and must say so rather than
        // reporting a success that did not occur.
        assert!(matches!(
            execute(&ctx, Command::PomodoroPause).unwrap(),
            CommandResult::Flag(false)
        ));
        assert!(matches!(
            execute(&ctx, Command::PomodoroStop).unwrap(),
            CommandResult::Flag(false)
        ));
    }

    #[test]
    fn a_pomodoro_runs_on_a_locked_list_and_cannot_switch_it_off() {
        for scheduled in [false, true] {
            let ctx = ctx();
            let list = if scheduled {
                scheduled_list(&ctx, None)
            } else {
                let list = create(&ctx, "Deep Work");
                protect(&ctx, list.id).unwrap();
                list
            };

            execute(
                &ctx,
                Command::PomodoroStart {
                    block_list_id: list.id,
                    config: PomodoroConfig::CLASSIC,
                },
            )
            .unwrap();
            // Into the break, which switches an unlocked list off, and then out
            // of the session, which puts a list back as it found it.
            execute(&ctx, Command::PomodoroSkip).unwrap();
            execute(&ctx, Command::PomodoroStop).unwrap();

            let engine = ctx.engine.lock().unwrap();
            assert!(engine.db().get_block_list(list.id).unwrap().enabled);
            assert!(engine.is_block_list_protected(list.id));
        }
    }

    fn set(ctx: &AppContext, key: &str, value: &str) -> CommandOutcome<CommandResult> {
        execute(
            ctx,
            Command::SetSetting {
                key: key.into(),
                value: value.into(),
            },
        )
    }

    #[test]
    fn a_lock_refuses_settings_that_would_loosen_browser_closing() {
        let ctx = ctx();
        let list = create(&ctx, "Deep Work");
        protect(&ctx, list.id).unwrap();

        // #18: switching this off let the extension be removed mid-lock.
        for (key, value) in [
            (SETTING_CLOSE_BROWSERS, "false"),
            (SETTING_GRACE_PERIOD, "3600"),
        ] {
            assert!(
                matches!(set(&ctx, key, value), Err(CommandError::Protected)),
                "{key}={value} got through a lock"
            );
        }
        assert!(matches!(
            execute(&ctx, Command::ResetSettings),
            Err(CommandError::Protected)
        ));

        set(&ctx, SETTING_CLOSE_BROWSERS, "true").unwrap();
        set(&ctx, SETTING_GRACE_PERIOD, "10").unwrap();
        set(&ctx, "language", "de").unwrap();
    }

    #[test]
    fn without_a_lock_browser_settings_change_freely() {
        let ctx = ctx();
        create(&ctx, "Deep Work");

        set(&ctx, SETTING_CLOSE_BROWSERS, "false").unwrap();
        set(&ctx, SETTING_GRACE_PERIOD, "3600").unwrap();
        execute(&ctx, Command::ResetSettings).unwrap();
    }

    #[test]
    fn a_lock_freezes_allowances_and_unblock_everything() {
        // The same for a lock set by hand and for one that follows the schedule.
        for scheduled in [false, true] {
            let ctx = ctx();
            let allow_youtube = || {
                execute(
                    &ctx,
                    Command::AllowanceCreate {
                        target: AllowanceMatch::Domain("youtube.com".into()),
                        daily_limit_secs: 600,
                        strict_mode: false,
                    },
                )
            };
            // Made before the lock, so there is one to change and reset.
            let CommandResult::Allowance(existing) = allow_youtube().unwrap() else {
                panic!("expected the created allowance back");
            };
            if scheduled {
                scheduled_list(&ctx, None);
            } else {
                let list = create(&ctx, "Deep Work");
                protect(&ctx, list.id).unwrap();
            }

            // An allowance with time left exempts its site from every list, so
            // making one was a way straight through a lock.
            let loosening = [
                allow_youtube(),
                execute(
                    &ctx,
                    Command::AllowanceUpdate {
                        id: existing.id,
                        daily_limit_secs: 86_400,
                        strict_mode: false,
                        enabled: true,
                    },
                ),
                execute(&ctx, Command::AllowanceResetToday { id: existing.id }),
                execute(&ctx, Command::RemoveBlocks),
            ];
            for attempt in loosening {
                assert!(matches!(attempt, Err(CommandError::Protected)));
            }

            // Deleting one only takes an exemption away.
            execute(&ctx, Command::AllowanceDelete { id: existing.id }).unwrap();
        }
    }

    #[test]
    fn allowance_create_list_and_delete() {
        let ctx = ctx();

        let created = execute(
            &ctx,
            Command::AllowanceCreate {
                target: AllowanceMatch::Domain("youtube.com".into()),
                daily_limit_secs: 600,
                strict_mode: true,
            },
        )
        .unwrap();
        let CommandResult::Allowance(allowance) = created else {
            panic!("expected the created allowance back");
        };

        let listed = execute(&ctx, Command::AllowanceList).unwrap();
        let CommandResult::Allowances(statuses) = listed else {
            panic!("expected allowance statuses");
        };
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].remaining_secs, 600, "nothing used yet");
        assert!(!statuses[0].exhausted);

        execute(&ctx, Command::AllowanceDelete { id: allowance.id }).unwrap();
        let after = execute(&ctx, Command::AllowanceList).unwrap();
        assert!(matches!(after, CommandResult::Allowances(v) if v.is_empty()));
    }

    #[test]
    fn allowance_rejects_limits_outside_the_supported_range() {
        let ctx = ctx();

        for bad in [30, 90_000] {
            let err = execute(
                &ctx,
                Command::AllowanceCreate {
                    target: AllowanceMatch::Domain("x.com".into()),
                    daily_limit_secs: bad,
                    strict_mode: false,
                },
            )
            .unwrap_err();
            assert_eq!(err.code(), "validation", "{bad}s should be rejected");
        }
    }

    #[test]
    fn updating_a_missing_allowance_reports_not_found() {
        let ctx = ctx();

        let err = execute(
            &ctx,
            Command::AllowanceUpdate {
                id: EntityId::new_v4(),
                daily_limit_secs: 600,
                strict_mode: false,
                enabled: true,
            },
        )
        .unwrap_err();

        assert_eq!(err.code(), "allowance_not_found");
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn deleting_a_missing_list_is_an_error_not_a_silent_success() {
        let ctx = ctx();
        let err = execute(
            &ctx,
            Command::DeleteBlockList {
                id: EntityId::new_v4(),
            },
        )
        .unwrap_err();

        assert_ne!(
            err.exit_code(),
            0,
            "a missing list must not report success to a script"
        );
    }

    // ─── Whole-configuration and diagnostics ──────────────────

    fn text(result: CommandResult) -> String {
        match result {
            CommandResult::Text(t) => t,
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn export_then_import_restores_the_same_lists() {
        let source = ctx();
        let list = create(&source, "Social media");
        execute(
            &source,
            Command::AddWebsiteRule {
                list_id: list.id,
                rule: WebsiteMatchType::Domain("reddit.com".into()),
            },
        )
        .unwrap();

        let json = text(execute(&source, Command::ExportConfiguration).unwrap());

        let target = ctx();
        create(&target, "Something else");
        let imported = execute(&target, Command::ImportConfiguration { json })
            .unwrap()
            .as_count()
            .unwrap();

        assert_eq!(imported, 1);
        let lists = lists(&target);
        assert_eq!(lists.len(), 1, "import replaces rather than merges");
        assert_eq!(lists[0].name, "Social media");
        assert_eq!(lists[0].websites.len(), 1);
    }

    #[test]
    fn import_rejects_a_document_from_a_newer_format() {
        let ctx = ctx();
        let json = r#"{"version":99,"app":"Focuser","exported_at":"2026-07-27T00:00:00Z","block_lists":[]}"#;

        let err = execute(
            &ctx,
            Command::ImportConfiguration {
                json: json.to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(err.code(), "validation");
    }

    #[test]
    fn import_rejects_junk_without_touching_existing_lists() {
        let ctx = ctx();
        create(&ctx, "Keep me");

        let err = execute(
            &ctx,
            Command::ImportConfiguration {
                json: "not json at all".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(err.code(), "validation");
        assert_eq!(
            lists(&ctx).len(),
            1,
            "a failed import must not delete anything"
        );
    }

    #[test]
    fn import_and_delete_all_are_refused_while_a_list_is_locked() {
        let ctx = ctx();
        let list = create(&ctx, "Locked");
        execute(
            &ctx,
            Command::EnableProtection {
                list_id: list.id,
                duration_minutes: Some(60),
                prevent_uninstall: true,
                prevent_service_stop: true,
                prevent_modification: true,
                lock: None,
            },
        )
        .unwrap();

        for cmd in [
            Command::DeleteAllData,
            Command::ImportConfiguration {
                json: r#"{"version":1,"app":"Focuser","exported_at":"2026-07-27T00:00:00Z","block_lists":[]}"#.to_string(),
            },
        ] {
            assert_eq!(execute(&ctx, cmd).unwrap_err().code(), "protected");
        }

        assert_eq!(lists(&ctx).len(), 1);
    }

    #[test]
    fn delete_all_data_empties_lists_and_statistics() {
        let ctx = ctx();
        create(&ctx, "Gone");

        execute(&ctx, Command::DeleteAllData).unwrap();

        assert!(lists(&ctx).is_empty());
    }

    #[test]
    fn check_domain_reports_blocked_only_for_enabled_lists() {
        let ctx = ctx();
        let list = create(&ctx, "Social media");
        execute(
            &ctx,
            Command::AddWebsiteRule {
                list_id: list.id,
                rule: WebsiteMatchType::Domain("reddit.com".into()),
            },
        )
        .unwrap();

        let blocked = |domain: &str| match execute(
            &ctx,
            Command::CheckDomain {
                domain: domain.to_string(),
            },
        )
        .unwrap()
        {
            CommandResult::Flag(b) => b,
            other => panic!("expected a flag, got {other:?}"),
        };

        assert!(blocked("reddit.com"));
        assert!(blocked("www.reddit.com"), "www should be stripped");
        assert!(!blocked("example.com"));

        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: false,
            },
        )
        .unwrap();

        assert!(!blocked("reddit.com"), "a disabled list blocks nothing");
    }

    #[test]
    fn browser_status_lists_every_known_browser_as_absent_when_headless() {
        let ctx = ctx();
        let result = execute(&ctx, Command::GetBrowserStatus).unwrap();

        let CommandResult::BrowserStatus(statuses) = result else {
            panic!("expected browser status");
        };
        assert_eq!(
            statuses.len(),
            focuser_common::browser::KNOWN_BROWSERS.len()
        );
        assert!(
            statuses
                .iter()
                .all(|s| !s.running && !s.extension_connected)
        );
    }

    #[test]
    fn app_icons_answer_for_every_target_even_when_there_is_no_icon() {
        let ctx = ctx();
        let result = execute(
            &ctx,
            Command::GetAppIcons {
                targets: vec!["Solitaire".into(), "definitely-not-installed.exe".into()],
            },
        )
        .unwrap();

        let CommandResult::AppIcons(icons) = result else {
            panic!("expected app icons");
        };
        // A row still needs an answer so the caller knows to fall back rather
        // than sit on a spinner.
        assert_eq!(icons.len(), 2);
        assert!(icons.iter().all(|i| i.data_uri.is_none()));
    }

    #[test]
    fn app_icons_are_read_once_per_distinct_target() {
        let ctx = ctx();
        let result = execute(
            &ctx,
            Command::GetAppIcons {
                targets: vec!["dupe.exe".into(), "dupe.exe".into(), "other.exe".into()],
            },
        )
        .unwrap();

        let CommandResult::AppIcons(icons) = result else {
            panic!("expected app icons");
        };
        assert_eq!(
            icons.iter().map(|i| i.target.as_str()).collect::<Vec<_>>(),
            ["dupe.exe", "other.exe"]
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_program_that_is_installed_comes_back_with_its_icon() {
        let ctx = ctx();
        let result = execute(
            &ctx,
            Command::GetAppIcons {
                targets: vec!["notepad.exe".into()],
            },
        )
        .unwrap();

        let CommandResult::AppIcons(icons) = result else {
            panic!("expected app icons");
        };
        assert!(
            icons[0]
                .data_uri
                .as_deref()
                .is_some_and(|uri| uri.starts_with("data:image/png;base64,"))
        );
    }

    #[test]
    fn app_version_is_the_crate_version() {
        let ctx = ctx();
        assert_eq!(
            text(execute(&ctx, Command::AppVersion).unwrap()),
            env!("CARGO_PKG_VERSION")
        );
    }

    // ─── www. and subdomains ──────────────────────────────────
    //
    // Blocking `youtube.com` used to leave `www.youtube.com` reachable in some
    // paths, and an allowance on `www.youtube.com` released nothing at all,
    // because every layer handled the prefix differently.

    #[test]
    fn a_domain_rule_covers_www_and_subdomains_whichever_form_was_typed() {
        for typed in [
            "youtube.com",
            "www.youtube.com",
            "https://www.youtube.com/feed",
        ] {
            let ctx = ctx();
            let list = create(&ctx, "Videos");
            execute(
                &ctx,
                Command::AddWebsiteRule {
                    list_id: list.id,
                    rule: WebsiteMatchType::Domain(typed.into()),
                },
            )
            .unwrap();

            for host in [
                "youtube.com",
                "www.youtube.com",
                "m.youtube.com",
                "music.youtube.com",
                "WWW.YouTube.com",
            ] {
                let CommandResult::Flag(blocked) = execute(
                    &ctx,
                    Command::CheckDomain {
                        domain: host.to_string(),
                    },
                )
                .unwrap() else {
                    panic!("expected a flag")
                };
                assert!(blocked, "rule {typed:?} should block {host:?}");
            }

            let CommandResult::Flag(unrelated) = execute(
                &ctx,
                Command::CheckDomain {
                    domain: "notyoutube.com".to_string(),
                },
            )
            .unwrap() else {
                panic!("expected a flag")
            };
            assert!(!unrelated, "rule {typed:?} must not block notyoutube.com");
        }
    }

    #[test]
    fn an_allowance_releases_the_domain_whichever_form_either_side_used() {
        // The reported bug: an allowance stored as `www.youtube.com` did not
        // release a block on `youtube.com`.
        for allowance_form in ["youtube.com", "www.youtube.com"] {
            for rule_form in ["youtube.com", "www.youtube.com"] {
                let ctx = ctx_with_extension();
                let list = create(&ctx, "Videos");
                execute(
                    &ctx,
                    Command::AddWebsiteRule {
                        list_id: list.id,
                        rule: WebsiteMatchType::Domain(rule_form.into()),
                    },
                )
                .unwrap();
                execute(
                    &ctx,
                    Command::AllowanceCreate {
                        target: focuser_common::allowance::AllowanceMatch::Domain(
                            allowance_form.into(),
                        ),
                        daily_limit_secs: 1800,
                        strict_mode: true,
                    },
                )
                .unwrap();

                for host in ["youtube.com", "www.youtube.com", "music.youtube.com"] {
                    let CommandResult::Flag(blocked) = execute(
                        &ctx,
                        Command::CheckDomain {
                            domain: host.to_string(),
                        },
                    )
                    .unwrap() else {
                        panic!("expected a flag")
                    };
                    assert!(
                        !blocked,
                        "allowance on {allowance_form:?} should release {host:?}                          against a rule on {rule_form:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_unmeasurable_allowance_does_not_become_an_unlimited_pass() {
        // Nothing measures browser time without the extension, so the usage
        // clock never starts. Granting the exemption anyway would mean a
        // 30-minute budget silently allowed the site all day.
        let ctx = ctx();
        let list = create(&ctx, "Videos");
        execute(
            &ctx,
            Command::AddWebsiteRule {
                list_id: list.id,
                rule: WebsiteMatchType::Domain("youtube.com".into()),
            },
        )
        .unwrap();
        execute(
            &ctx,
            Command::AllowanceCreate {
                target: focuser_common::allowance::AllowanceMatch::Domain("youtube.com".into()),
                daily_limit_secs: 1800,
                strict_mode: true,
            },
        )
        .unwrap();

        // `ctx()` is headless — `connected_browsers()` is empty, like a machine
        // with no extension installed.
        let CommandResult::Flag(blocked) = execute(
            &ctx,
            Command::CheckDomain {
                domain: "www.youtube.com".to_string(),
            },
        )
        .unwrap() else {
            panic!("expected a flag")
        };

        assert!(
            blocked,
            "with no extension to measure usage the site must stay blocked"
        );
    }

    #[test]
    fn blocking_health_reports_a_machine_that_cannot_block_at_all() {
        let ctx = ctx_without_hosts_access();
        let list = create(&ctx, "Social");
        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: true,
            },
        )
        .unwrap();

        let CommandResult::BlockingHealth(health) =
            execute(&ctx, Command::GetBlockingHealth).unwrap()
        else {
            panic!("expected blocking health");
        };

        assert_eq!(health.active_lists, 1);
        assert!(!health.extension_connected);
        assert!(!health.hosts_writable);
        assert!(
            health.is_failing(),
            "an enabled list with neither mechanism available is a failure"
        );
    }

    #[test]
    fn blocking_health_is_calm_when_the_extension_is_doing_the_work() {
        let ctx = ctx_with_extension();
        let list = create(&ctx, "Social");
        execute(
            &ctx,
            Command::ToggleBlockList {
                id: list.id,
                enabled: true,
            },
        )
        .unwrap();

        let CommandResult::BlockingHealth(health) =
            execute(&ctx, Command::GetBlockingHealth).unwrap()
        else {
            panic!("expected blocking health");
        };

        assert!(health.extension_connected);
        assert!(!health.is_failing());
    }
}
