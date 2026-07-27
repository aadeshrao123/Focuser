//! Rendering a [`CommandResult`] for a terminal.
//!
//! Human output is for people and is explicitly **not** a stable interface —
//! scripts should use `--json`, which serialises the result verbatim.

use focuser_app::CommandResult;

/// Print a result as JSON. Used for `--json`.
pub fn print_json(result: &CommandResult) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(result)?);
    Ok(())
}

/// Print a result for a human reader.
pub fn print_human(result: &CommandResult) {
    match result {
        CommandResult::Unit => println!("OK"),

        CommandResult::Count(n) => println!("{n}"),

        CommandResult::Flag(b) => println!("{b}"),

        CommandResult::Setting(Some(v)) => println!("{v}"),
        CommandResult::Setting(None) => println!("(unset)"),

        CommandResult::BlockList(list) => {
            println!("{}  {}", list.id, list.name);
        }

        CommandResult::BlockLists(lists) if lists.is_empty() => {
            println!("No block lists.");
        }
        CommandResult::BlockLists(lists) => {
            for l in lists {
                println!(
                    "{}  {:<24} {:<8} {} sites, {} apps, {} exceptions",
                    l.id,
                    truncate(&l.name, 24),
                    if l.enabled { "enabled" } else { "disabled" },
                    l.websites.len(),
                    l.applications.len(),
                    l.exceptions.len(),
                );
            }
        }

        CommandResult::WebsiteRule(rule) => println!("{}", rule.id),
        CommandResult::AppRule(rule) => println!("{}", rule.id),
        CommandResult::Exception(exc) => println!("{}", exc.id),

        CommandResult::Stats(stats) if stats.is_empty() => println!("No statistics."),
        CommandResult::Stats(stats) => {
            for s in stats {
                println!(
                    "{}  {:<32} {:>8}s {:>6} blocked",
                    s.date,
                    truncate(&s.domain_or_app, 32),
                    s.duration_seconds,
                    s.blocked_attempts
                );
            }
        }

        CommandResult::BlockedEvents(events) if events.is_empty() => println!("No events."),
        CommandResult::BlockedEvents(events) => {
            for e in events {
                println!("{}  {}", e.timestamp, e.domain_or_app);
            }
        }

        CommandResult::ProtectionStatus(infos) if infos.is_empty() => {
            println!("No active protection.");
        }
        CommandResult::ProtectionStatus(infos) => {
            for i in infos {
                println!(
                    "{}  {:<24} {} remaining (until {})",
                    i.block_list_id,
                    truncate(&i.block_list_name, 24),
                    format_duration(i.remaining_seconds),
                    i.expires_at.to_rfc3339(),
                );
            }
        }

        CommandResult::PomodoroStatus(None) => println!("No session running."),
        CommandResult::PomodoroStatus(Some(status)) => {
            println!(
                "{} — {} remaining, cycle {} of {}{}  [{}]",
                status.current_phase.as_str(),
                format_duration(u64::from(status.remaining_secs)),
                status.current_cycle,
                status.config.cycles_until_long_break,
                if status.paused { " (paused)" } else { "" },
                status.block_list_name,
            );
        }

        CommandResult::PomodoroSession(session) => println!("{}", session.id),

        CommandResult::PomodoroEvents(events) if events.is_empty() => println!("No events."),
        CommandResult::PomodoroEvents(events) => {
            for e in events {
                match e {
                    focuser_app::command::PomodoroEventDto::PhaseAdvanced { to, cycle } => {
                        println!("phase advanced to {to} (cycle {cycle})");
                    }
                    focuser_app::command::PomodoroEventDto::TamperDetected => {
                        println!("tamper detected");
                    }
                }
            }
        }

        CommandResult::PomodoroHistory(entries) if entries.is_empty() => println!("No sessions."),
        CommandResult::PomodoroHistory(entries) => {
            for e in entries {
                println!(
                    "{}  {} cycles, {} of work",
                    e.started_at.to_rfc3339(),
                    e.completed_cycles,
                    format_duration(u64::from(e.total_work_secs)),
                );
            }
        }

        CommandResult::PomodoroPresets(presets) => {
            for p in presets {
                println!(
                    "{:<8} {:<8} {} focus / {} short / {} long, long break every {}",
                    p.key,
                    p.label,
                    format_duration(u64::from(p.config.work_secs)),
                    format_duration(u64::from(p.config.short_break_secs)),
                    format_duration(u64::from(p.config.long_break_secs)),
                    p.config.cycles_until_long_break,
                );
            }
        }

        CommandResult::Allowance(a) => println!("{}  {}", a.id, a.target.display()),

        CommandResult::Allowances(statuses) if statuses.is_empty() => println!("No allowances."),
        CommandResult::Allowances(statuses) => {
            for s in statuses {
                println!(
                    "{}  {:<32} {} used of {} {}",
                    s.allowance.id,
                    truncate(s.allowance.target.display(), 32),
                    format_duration(u64::from(s.used_today_secs)),
                    format_duration(u64::from(s.allowance.daily_limit_secs)),
                    if s.exhausted { "(exhausted)" } else { "" },
                );
            }
        }

        CommandResult::AllowanceNotifications(ns) if ns.is_empty() => {
            println!("No notifications.");
        }
        CommandResult::AllowanceNotifications(ns) => {
            for n in ns {
                println!(
                    "{}  {} — {} of {}",
                    n.kind,
                    n.target,
                    format_duration(u64::from(n.used_secs)),
                    format_duration(u64::from(n.limit_secs)),
                );
            }
        }

        CommandResult::AllowanceHistory(entries) if entries.is_empty() => println!("No usage."),
        CommandResult::AllowanceHistory(entries) => {
            for e in entries {
                println!("{}  {}", e.date, format_duration(u64::from(e.used_secs)));
            }
        }

        // Exported JSON goes to stdout verbatim so it can be redirected to a file.
        CommandResult::Text(text) => println!("{text}"),

        CommandResult::BrowserStatus(browsers) => {
            for b in browsers {
                println!(
                    "{:<12} {:<8} {}",
                    truncate(&b.display_name, 12),
                    if b.running { "running" } else { "-" },
                    if b.extension_connected {
                        "extension connected"
                    } else {
                        "no extension"
                    },
                );
            }
        }

        // A terminal cannot show the image, so report whether one was found.
        // `--json` carries the data URI itself for anything that wants it.
        CommandResult::AppIcons(icons) => {
            for i in icons {
                println!(
                    "{:<32} {}",
                    truncate(&i.target, 32),
                    match &i.data_uri {
                        Some(uri) => format!("icon ({} bytes)", uri.len()),
                        None => "no icon".to_string(),
                    },
                );
            }
        }
    }
}

/// `3h 5m`, `5m 30s`, `30s`. Compact enough for a table column.
fn format_duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, _) => format!("{m}m {s}s"),
        _ => format!("{h}h {m}m"),
    }
}

/// Keep table columns aligned when a name is longer than its column.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations_by_magnitude() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(330), "5m 30s");
        assert_eq!(format_duration(11_100), "3h 5m");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        // Multi-byte input must not panic or split a character.
        assert_eq!(truncate("日本語のテキスト", 4), "日本語…");
    }
}
