//! Command dispatch — the single place application behaviour lives.
//!
//! The `match` is exhaustive with no `_` arm on purpose: adding a [`Command`]
//! variant without handling it here is a compile error.

use focuser_common::allowance::Allowance;
use focuser_common::types::{
    AppRule, BlockList, EntityId, ExceptionRule, Protection, Schedule, WebsiteMatchType,
    WebsiteRule,
};
use focuser_core::{BlockEngine, pomodoro};

use crate::command::{
    AllowanceNotificationDto, AllowanceUsageEntry, Command, CommandResult, PomodoroEventDto,
    PomodoroHistoryEntry, ProtectionInfo,
};
use crate::context::{AppContext, PomodoroEvent};
use crate::error::{CommandError, CommandOutcome};

/// Run one command against the shared context.
pub fn execute(ctx: &AppContext, cmd: Command) -> CommandOutcome<CommandResult> {
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
            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::DeleteBlockList { id } => {
            ensure_unprotected(&engine, id)?;
            engine.db().delete_block_list(id)?;
            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::ToggleBlockList { id, enabled } => {
            // Protection guards *disabling* only — turning blocking back on is
            // always allowed, since it cannot be used to escape a commitment.
            if !enabled {
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
            let created = WebsiteRule {
                id: focuser_common::types::new_id(),
                match_type: rule,
                enabled: true,
            };
            let out = created.clone();
            mutate_list(ctx, &mut engine, list_id, |list| {
                list.websites.push(created);
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
                    if list
                        .websites
                        .iter()
                        .any(|r| website_value(r) == Some(&value))
                    {
                        continue;
                    }
                    list.websites.push(kind.rule(&value));
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
            let created = ExceptionRule {
                id: focuser_common::types::new_id(),
                exception_type: exception,
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
        } => {
            if duration_minutes == 0 {
                return Err(CommandError::Validation(
                    "protection duration must be at least 1 minute".into(),
                ));
            }

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
                expires_at: now + chrono::Duration::minutes(i64::from(duration_minutes)),
            });
            // Protecting a disabled list would protect nothing.
            list.enabled = true;
            list.updated_at = now;

            engine.db().update_block_list(&list)?;
            engine.refresh()?;
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::GetProtectionStatus => {
            let infos = engine
                .block_lists()
                .iter()
                .filter(|l| l.has_active_protection())
                .filter_map(|l| {
                    let p = l.protection.as_ref()?;
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
            engine.db().set_setting(&key, &value)?;
            Ok(CommandResult::Unit)
        }

        Command::ResetSettings => {
            engine.db().clear_settings()?;
            Ok(CommandResult::Unit)
        }

        // ─── Enforcement ──────────────────────────────────────────
        Command::ApplyBlocks => {
            ctx.sync_hosts(&engine);
            Ok(CommandResult::Unit)
        }

        Command::RemoveBlocks => {
            // An empty domain set is how "unblock everything" is expressed —
            // the sync writes the list, so an empty list clears the section.
            ctx.sync_hosts_with(&[]);
            Ok(CommandResult::Unit)
        }

        // ─── Pomodoro ─────────────────────────────────────────────
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
            engine.db().list_allowance_statuses()?,
        )),

        Command::AllowanceCreate {
            target,
            daily_limit_secs,
            strict_mode,
        } => {
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
            engine.db().delete_allowance(id)?;
            ctx.allowance_tracker.rebuild_from_db(engine.db())?;
            Ok(CommandResult::Unit)
        }

        Command::AllowanceResetToday { id } => {
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
    }
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
        execute(
            ctx,
            Command::EnableProtection {
                list_id: id,
                duration_minutes: 60,
                prevent_uninstall: true,
                prevent_service_stop: true,
                prevent_modification: true,
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
        assert!(infos[0].remaining_seconds > 0);
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
}
