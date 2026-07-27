//! CLI surface — a clap tree that maps one-to-one onto [`Command`].
//!
//! Every subcommand does exactly one thing: build a `Command`. There is no logic
//! here, which is what keeps the CLI and the GUI from diverging — both are thin
//! shims over the same `execute()`.

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};
use focuser_app::Command;
use focuser_common::allowance::AllowanceMatch;
use focuser_common::pomodoro::PomodoroConfig;
use focuser_common::types::{AppMatchType, EntityId, ExceptionType, TimeSlot, WebsiteMatchType};

use crate::slot::SlotSpec;

#[derive(Parser)]
#[command(
    name = "focuser",
    about = "Focuser — website and app blocker",
    version,
    // Show a useful error instead of silently doing nothing.
    arg_required_else_help = true
)]
pub struct Cli {
    /// Emit the raw result as JSON. Use this for scripting; the human-readable
    /// output is not a stable interface.
    #[arg(long, global = true)]
    pub json: bool,

    /// Database file to operate on. Defaults to the app's real database.
    ///
    /// Point this at a scratch file to exercise commands without touching real
    /// block lists — this is what makes the CLI usable as a test harness.
    #[arg(long, global = true, value_name = "PATH")]
    pub db: Option<PathBuf>,

    #[command(subcommand)]
    pub command: TopLevel,
}

#[derive(Subcommand)]
pub enum TopLevel {
    /// Block lists.
    #[command(subcommand)]
    List(ListCmd),
    /// Website rules.
    #[command(subcommand)]
    Site(SiteCmd),
    /// Application rules.
    #[command(subcommand)]
    App(AppCmd),
    /// Exceptions (allow-list entries within a block list).
    #[command(subcommand)]
    Exception(ExceptionCmd),
    /// Weekly schedule.
    #[command(subcommand)]
    Schedule(ScheduleCmd),
    /// Usage statistics.
    #[command(subcommand)]
    Stats(StatsCmd),
    /// Protection windows.
    #[command(subcommand)]
    Protect(ProtectCmd),
    /// Key/value settings.
    #[command(subcommand)]
    Setting(SettingCmd),
    /// Hosts-file enforcement.
    #[command(subcommand)]
    Blocks(BlocksCmd),
    /// Pomodoro sessions.
    #[command(subcommand)]
    Pomodoro(PomodoroCmd),
    /// Daily allowances.
    #[command(subcommand)]
    Allowance(AllowanceCmd),
    /// Whole-configuration export, import and wipe.
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Is this domain blocked right now?
    Check { domain: String },
    /// Which browsers are running, and which have the extension.
    Browsers,
    /// Print the application version.
    Version,
}

#[derive(Subcommand)]
pub enum ConfigCmd {
    /// Write every block list to stdout as JSON.
    Export,
    /// Replace every block list with the contents of a file. `-` reads stdin.
    Import { path: std::path::PathBuf },
    /// Delete every block list, rule, schedule, statistic and setting.
    Wipe {
        /// Required. Without it nothing is deleted.
        #[arg(long)]
        yes: bool,
    },
}

// ─── Block lists ────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum ListCmd {
    /// Show every block list.
    Ls,
    /// Create an empty block list.
    Create { name: String },
    /// Delete a block list.
    Rm { id: EntityId },
    /// Turn a block list on.
    Enable { id: EntityId },
    /// Turn a block list off.
    Disable { id: EntityId },
}

// ─── Website rules ──────────────────────────────────────────────────

#[derive(Copy, Clone, ValueEnum)]
pub enum SiteKind {
    Domain,
    Keyword,
    Wildcard,
    UrlPath,
}

#[derive(Subcommand)]
pub enum SiteCmd {
    /// Add a website rule.
    Add {
        list_id: EntityId,
        value: String,
        #[arg(long, value_enum, default_value_t = SiteKind::Domain)]
        kind: SiteKind,
    },
    /// Block every site except the list's exceptions.
    AddEntireInternet { list_id: EntityId },
    /// Remove a website rule.
    Rm {
        list_id: EntityId,
        rule_id: EntityId,
    },
    /// Add many rules at once. Blank lines and `#` comments are skipped.
    Import {
        list_id: EntityId,
        /// Read values from a file, one per line. Use `-` for stdin.
        #[arg(long, conflicts_with = "values")]
        file: Option<PathBuf>,
        /// Values given directly on the command line.
        values: Vec<String>,
        #[arg(long, value_enum, default_value_t = SiteKind::Domain)]
        kind: SiteKind,
    },
    /// Remove every website rule from every unprotected list.
    Clear,
}

// ─── Application rules ──────────────────────────────────────────────

#[derive(Copy, Clone, ValueEnum)]
pub enum AppKind {
    ExeName,
    ExePath,
    WindowTitle,
}

#[derive(Subcommand)]
pub enum AppCmd {
    /// Add an application rule.
    Add {
        list_id: EntityId,
        value: String,
        #[arg(long, value_enum, default_value_t = AppKind::ExeName)]
        kind: AppKind,
    },
    /// Remove an application rule.
    Rm {
        list_id: EntityId,
        rule_id: EntityId,
    },
    /// Remove every application rule from every unprotected list.
    Clear,
}

// ─── Exceptions ─────────────────────────────────────────────────────

#[derive(Copy, Clone, ValueEnum)]
pub enum ExceptionKind {
    Domain,
    Wildcard,
    LocalFiles,
}

#[derive(Subcommand)]
pub enum ExceptionCmd {
    Add {
        list_id: EntityId,
        /// Ignored for `local-files`.
        #[arg(default_value = "")]
        value: String,
        #[arg(long, value_enum, default_value_t = ExceptionKind::Domain)]
        kind: ExceptionKind,
    },
    Rm {
        list_id: EntityId,
        exception_id: EntityId,
    },
}

// ─── Schedule ───────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum ScheduleCmd {
    /// Replace a list's schedule.
    Set {
        list_id: EntityId,
        /// Block at all times, clearing any schedule.
        #[arg(long, conflicts_with = "slot")]
        always: bool,
        /// A slot as `DAY:START-END`, e.g. `mon:09:00-17:00`. Repeatable.
        #[arg(long = "slot", value_name = "DAY:START-END")]
        slot: Vec<SlotSpec>,
    },
}

// ─── Statistics ─────────────────────────────────────────────────────

#[derive(Args)]
pub struct DateRange {
    /// Start date, `YYYY-MM-DD`. Defaults to today.
    #[arg(long)]
    pub from: Option<NaiveDate>,
    /// End date, `YYYY-MM-DD`. Defaults to today.
    #[arg(long)]
    pub to: Option<NaiveDate>,
}

impl DateRange {
    fn resolve(&self) -> (NaiveDate, NaiveDate) {
        let today = chrono::Local::now().date_naive();
        (self.from.unwrap_or(today), self.to.unwrap_or(today))
    }
}

#[derive(Subcommand)]
pub enum StatsCmd {
    /// Aggregated usage per domain or app.
    Show(DateRange),
    /// Individual blocked-attempt events.
    Events(DateRange),
    /// Delete all statistics. Block lists are preserved.
    Clear,
    /// Show the retention window in days.
    Retention,
    /// Set the retention window and prune anything older.
    SetRetention { days: u32 },
}

// ─── Protection ─────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum ProtectCmd {
    /// Start a protection window on a block list.
    ///
    /// All three protections are on by default — a protection window that
    /// protects nothing is never what "enable protection" means. Use the
    /// `--allow-*` flags to opt out of individual ones.
    Enable {
        id: EntityId,
        #[arg(long, default_value_t = 60)]
        minutes: u32,
        /// Permit uninstalling Focuser while protection is active.
        #[arg(long)]
        allow_uninstall: bool,
        /// Permit stopping the Focuser service while protection is active.
        #[arg(long)]
        allow_service_stop: bool,
        /// Permit editing this block list while protection is active.
        #[arg(long)]
        allow_modification: bool,
    },
    /// Show active protection windows.
    Status,
}

// ─── Settings ───────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum SettingCmd {
    Get {
        key: String,
        #[arg(long)]
        default: Option<String>,
    },
    Set {
        key: String,
        value: String,
    },
    /// Reset settings to defaults. Block lists and statistics are preserved.
    Reset,
}

// ─── Enforcement ────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum BlocksCmd {
    /// Write the current blocked domains to the hosts file.
    Apply,
    /// Remove Focuser's hosts-file entries.
    Remove,
}

// ─── Pomodoro ───────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum PomodoroCmd {
    Status,
    Start {
        block_list_id: EntityId,
        #[arg(long, default_value_t = 1500)]
        work_secs: u32,
        #[arg(long, default_value_t = 300)]
        short_break_secs: u32,
        #[arg(long, default_value_t = 900)]
        long_break_secs: u32,
        #[arg(long, default_value_t = 4)]
        cycles: u32,
    },
    Pause,
    Resume,
    Skip,
    Stop,
    /// Take and clear buffered phase-change events.
    Events,
    History {
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
}

// ─── Allowances ─────────────────────────────────────────────────────

#[derive(Copy, Clone, ValueEnum)]
pub enum AllowanceKind {
    Domain,
    App,
}

#[derive(Subcommand)]
pub enum AllowanceCmd {
    Ls,
    Create {
        value: String,
        #[arg(long, value_enum, default_value_t = AllowanceKind::Domain)]
        kind: AllowanceKind,
        /// Daily limit in seconds (60–86400).
        #[arg(long)]
        limit_secs: u32,
        /// Only count time while the tab or app is focused.
        #[arg(long)]
        strict: bool,
    },
    Update {
        id: EntityId,
        #[arg(long)]
        limit_secs: u32,
        #[arg(long)]
        strict: bool,
        #[arg(long, default_value_t = true)]
        enabled: bool,
    },
    Rm {
        id: EntityId,
    },
    /// Zero today's usage.
    Reset {
        id: EntityId,
    },
    /// Take and clear pending threshold notifications.
    Notifications,
    History {
        id: EntityId,
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
}

// ─── Mapping to Command ─────────────────────────────────────────────

impl TopLevel {
    /// Build the [`Command`] this invocation represents.
    ///
    /// Fallible only where an argument needs reading from disk.
    pub fn into_command(self) -> anyhow::Result<Command> {
        Ok(match self {
            TopLevel::List(c) => match c {
                ListCmd::Ls => Command::ListBlockLists,
                ListCmd::Create { name } => Command::CreateBlockList { name },
                ListCmd::Rm { id } => Command::DeleteBlockList { id },
                ListCmd::Enable { id } => Command::ToggleBlockList { id, enabled: true },
                ListCmd::Disable { id } => Command::ToggleBlockList { id, enabled: false },
            },

            TopLevel::Site(c) => match c {
                SiteCmd::Add {
                    list_id,
                    value,
                    kind,
                } => Command::AddWebsiteRule {
                    list_id,
                    rule: match kind {
                        SiteKind::Domain => WebsiteMatchType::Domain(value),
                        SiteKind::Keyword => WebsiteMatchType::Keyword(value),
                        SiteKind::Wildcard => WebsiteMatchType::Wildcard(value),
                        SiteKind::UrlPath => WebsiteMatchType::UrlPath(value),
                    },
                },
                SiteCmd::AddEntireInternet { list_id } => Command::AddWebsiteRule {
                    list_id,
                    rule: WebsiteMatchType::EntireInternet,
                },
                SiteCmd::Rm { list_id, rule_id } => Command::RemoveWebsiteRule { list_id, rule_id },
                SiteCmd::Import {
                    list_id,
                    file,
                    values,
                    kind,
                } => Command::BulkImportWebsites {
                    list_id,
                    values: match file {
                        Some(path) => read_lines(&path)?,
                        None => values,
                    },
                    kind: match kind {
                        SiteKind::Domain => focuser_app::command::WebsiteRuleKind::Domain,
                        SiteKind::Keyword => focuser_app::command::WebsiteRuleKind::Keyword,
                        SiteKind::Wildcard => focuser_app::command::WebsiteRuleKind::Wildcard,
                        SiteKind::UrlPath => focuser_app::command::WebsiteRuleKind::UrlPath,
                    },
                },
                SiteCmd::Clear => Command::ClearAllWebsites,
            },

            TopLevel::App(c) => match c {
                AppCmd::Add {
                    list_id,
                    value,
                    kind,
                } => Command::AddAppRule {
                    list_id,
                    rule: match kind {
                        AppKind::ExeName => AppMatchType::ExecutableName(value),
                        AppKind::ExePath => AppMatchType::ExecutablePath(value),
                        AppKind::WindowTitle => AppMatchType::WindowTitle(value),
                    },
                },
                AppCmd::Rm { list_id, rule_id } => Command::RemoveAppRule { list_id, rule_id },
                AppCmd::Clear => Command::ClearAllApps,
            },

            TopLevel::Exception(c) => match c {
                ExceptionCmd::Add {
                    list_id,
                    value,
                    kind,
                } => Command::AddException {
                    list_id,
                    exception: match kind {
                        ExceptionKind::Domain => ExceptionType::Domain(value),
                        ExceptionKind::Wildcard => ExceptionType::Wildcard(value),
                        ExceptionKind::LocalFiles => ExceptionType::LocalFiles,
                    },
                },
                ExceptionCmd::Rm {
                    list_id,
                    exception_id,
                } => Command::RemoveException {
                    list_id,
                    exception_id,
                },
            },

            TopLevel::Schedule(ScheduleCmd::Set {
                list_id,
                always,
                slot,
            }) => Command::UpdateSchedule {
                list_id,
                slots: slot.into_iter().map(TimeSlot::from).collect(),
                always_active: always,
            },

            TopLevel::Stats(c) => match c {
                StatsCmd::Show(range) => {
                    let (from, to) = range.resolve();
                    Command::GetStats { from, to }
                }
                StatsCmd::Events(range) => {
                    let (from, to) = range.resolve();
                    Command::GetBlockedEvents { from, to }
                }
                StatsCmd::Clear => Command::ClearStatistics,
                StatsCmd::Retention => Command::GetStatsRetention,
                StatsCmd::SetRetention { days } => Command::SetStatsRetention { days },
            },

            TopLevel::Protect(c) => match c {
                ProtectCmd::Enable {
                    id,
                    minutes,
                    allow_uninstall,
                    allow_service_stop,
                    allow_modification,
                } => Command::EnableProtection {
                    list_id: id,
                    duration_minutes: minutes,
                    prevent_uninstall: !allow_uninstall,
                    prevent_service_stop: !allow_service_stop,
                    prevent_modification: !allow_modification,
                },
                ProtectCmd::Status => Command::GetProtectionStatus,
            },

            TopLevel::Setting(c) => match c {
                SettingCmd::Get { key, default } => Command::GetSetting { key, default },
                SettingCmd::Set { key, value } => Command::SetSetting { key, value },
                SettingCmd::Reset => Command::ResetSettings,
            },

            TopLevel::Config(c) => match c {
                ConfigCmd::Export => Command::ExportConfiguration,
                ConfigCmd::Import { path } => Command::ImportConfiguration {
                    json: read_source(&path)?,
                },
                ConfigCmd::Wipe { yes } => {
                    if !yes {
                        anyhow::bail!("refusing to wipe everything without --yes");
                    }
                    Command::DeleteAllData
                }
            },

            TopLevel::Check { domain } => Command::CheckDomain { domain },
            TopLevel::Browsers => Command::GetBrowserStatus,
            TopLevel::Version => Command::AppVersion,

            TopLevel::Blocks(c) => match c {
                BlocksCmd::Apply => Command::ApplyBlocks,
                BlocksCmd::Remove => Command::RemoveBlocks,
            },

            TopLevel::Pomodoro(c) => match c {
                PomodoroCmd::Status => Command::PomodoroStatus,
                PomodoroCmd::Start {
                    block_list_id,
                    work_secs,
                    short_break_secs,
                    long_break_secs,
                    cycles,
                } => Command::PomodoroStart {
                    block_list_id,
                    config: PomodoroConfig {
                        work_secs,
                        short_break_secs,
                        long_break_secs,
                        cycles_until_long_break: cycles,
                    },
                },
                PomodoroCmd::Pause => Command::PomodoroPause,
                PomodoroCmd::Resume => Command::PomodoroResume,
                PomodoroCmd::Skip => Command::PomodoroSkip,
                PomodoroCmd::Stop => Command::PomodoroStop,
                PomodoroCmd::Events => Command::PomodoroDrainEvents,
                PomodoroCmd::History { days } => Command::PomodoroHistory { days },
            },

            TopLevel::Allowance(c) => match c {
                AllowanceCmd::Ls => Command::AllowanceList,
                AllowanceCmd::Create {
                    value,
                    kind,
                    limit_secs,
                    strict,
                } => Command::AllowanceCreate {
                    target: match kind {
                        AllowanceKind::Domain => AllowanceMatch::Domain(value),
                        AllowanceKind::App => AllowanceMatch::AppExecutable(value),
                    },
                    daily_limit_secs: limit_secs,
                    strict_mode: strict,
                },
                AllowanceCmd::Update {
                    id,
                    limit_secs,
                    strict,
                    enabled,
                } => Command::AllowanceUpdate {
                    id,
                    daily_limit_secs: limit_secs,
                    strict_mode: strict,
                    enabled,
                },
                AllowanceCmd::Rm { id } => Command::AllowanceDelete { id },
                AllowanceCmd::Reset { id } => Command::AllowanceResetToday { id },
                AllowanceCmd::Notifications => Command::AllowanceDrainNotifications,
                AllowanceCmd::History { id, days } => Command::AllowanceHistory { id, days },
            },
        })
    }
}

/// Read one value per line, from a file or from stdin when the path is `-`.
fn read_lines(path: &std::path::Path) -> anyhow::Result<Vec<String>> {
    // Blank and comment lines are filtered downstream by BulkImportWebsites,
    // so they are deliberately passed through rather than stripped twice.
    Ok(read_source(path)?.lines().map(str::to_string).collect())
}

/// Whole contents of a file, or of stdin when the path is `-`.
fn read_source(path: &std::path::Path) -> anyhow::Result<String> {
    use std::io::Read;

    if path == std::path::Path::new("-") {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        return Ok(buf);
    }
    Ok(std::fs::read_to_string(path)?)
}
