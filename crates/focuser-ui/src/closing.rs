//! Closing blocked apps without losing work (#65).
//!
//! A blocked program is first asked to close, as clicking X would, so an
//! editor can offer to save. Only one still running when its time is up is
//! closed by force. And a few minutes before a block starts or a time limit
//! runs out, the user is told which open apps it will close.

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use focuser_common::allowance::AllowanceMatch;
use focuser_common::browser::identify_browser;
use focuser_common::process::Process;
use focuser_common::types::{AppMatchType, AppRule, BlockList};
use focuser_core::BlockEngine;
use focuser_core::shared_allowance::{active_at, occurrence};
use tracing::info;

/// How long a program asked to close has to do it, "save changes?" and all.
const CLOSE_GRACE: Duration = Duration::from_secs(30);

/// How far ahead the user hears about the apps a block will close.
const WARN_AHEAD_SECS: i64 = 5 * 60;

/// The programs being closed, by name.
#[derive(Default)]
pub struct Closer {
    closing: HashMap<String, Stage>,
}

enum Stage {
    /// Asked to close at this time, and given until the grace is over.
    Asked(Instant),
    /// Closed by force. Still listed means it is on its way out, or a Unix
    /// program that ignored the signal and gets it again.
    Forced,
}

impl Closer {
    /// Close what is blocked right now, given as `(pid, name)`.
    ///
    /// All processes of one name go together. An editor can run several and
    /// only the main one has a window: closing the rest by force while it
    /// asks to save would lose the work anyway.
    ///
    /// Returns the names stopped for the first time.
    pub fn close(
        &mut self,
        blocked: &[(u32, String)],
        now: Instant,
        mut ask: impl FnMut(u32) -> bool,
        mut terminate: impl FnMut(u32) -> bool,
    ) -> Vec<String> {
        let mut apps: HashMap<String, (&str, Vec<u32>)> = HashMap::new();
        for (pid, name) in blocked {
            apps.entry(name.to_ascii_lowercase())
                .or_insert((name, Vec::new()))
                .1
                .push(*pid);
        }
        // Gone, or no longer blocked. If it starts again it is asked again.
        self.closing.retain(|key, _| apps.contains_key(key));

        let mut stopped = Vec::new();
        for (key, (name, pids)) in apps {
            let mut force = || pids.iter().for_each(|&pid| _ = terminate(pid));
            let next = match self.closing.get(&key) {
                Some(Stage::Asked(since)) if now.duration_since(*since) < CLOSE_GRACE => continue,
                Some(Stage::Asked(_)) => {
                    info!(name, "A blocked app did not close when asked, closing it");
                    force();
                    Stage::Forced
                }
                Some(Stage::Forced) => {
                    force();
                    continue;
                }
                None => {
                    stopped.push(name.to_string());
                    // Every one is asked: `any` would stop at the first.
                    if pids.iter().fold(false, |asked, &pid| ask(pid) | asked) {
                        info!(name, "Asked a blocked app to close");
                        Stage::Asked(now)
                    } else {
                        info!(name, "Closed a blocked app");
                        force();
                        Stage::Forced
                    }
                }
            };
            self.closing.insert(key, next);
        }
        stopped
    }
}

/// What the warning says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosingSoon {
    pub apps: Vec<String>,
    pub minutes: u32,
}

/// The warning waiting to be shown.
static PROMPT: Mutex<Option<ClosingSoon>> = Mutex::new(None);

pub fn prompt(soon: ClosingSoon) {
    if let Ok(mut slot) = PROMPT.lock() {
        *slot = Some(soon);
    }
}

pub fn take_prompt() -> Option<ClosingSoon> {
    PROMPT.lock().ok()?.take()
}

/// Warnings already given, and when.
#[derive(Default)]
pub struct Warner {
    warned: HashMap<String, DateTime<Utc>>,
}

impl Warner {
    /// The warning to give now, if a block a few minutes away will close an
    /// app that is open. Each block is warned about once.
    ///
    /// Paths and titles are only read when a block that is coming has a rule
    /// that needs them.
    pub fn check(
        &mut self,
        eng: &BlockEngine,
        procs: &[Process],
        now: DateTime<Local>,
        mut exe_path: impl FnMut(u32) -> Option<String>,
        titles: impl FnOnce() -> HashMap<u32, Vec<String>>,
    ) -> Option<ClosingSoon> {
        let now_utc = now.with_timezone(&Utc);
        self.warned
            .retain(|_, at| now_utc - *at < chrono::Duration::days(1));

        let soon: Vec<Soon> = upcoming(eng, now)
            .into_iter()
            .filter(|s| !self.warned.contains_key(&s.key))
            .collect();
        if soon.is_empty() {
            return None;
        }
        let wants_titles = soon.iter().any(|s| match s.closes {
            Closes::List(list) => has_rule(list, |m| matches!(m, AppMatchType::WindowTitle(_))),
            Closes::Program(_) => false,
        });
        let titles = if wants_titles {
            titles()
        } else {
            HashMap::new()
        };

        let mut apps = BTreeSet::new();
        let mut minutes = u32::MAX;
        for s in soon {
            let open = running(&s.closes, procs, &mut exe_path, &titles);
            // Nothing open yet. Something opened later still gets the warning.
            if open.is_empty() {
                continue;
            }
            apps.extend(open);
            let secs = (s.at - now_utc).num_seconds().max(0) as u64;
            minutes = minutes.min(secs.div_ceil(60).max(1) as u32);
            self.warned.insert(s.key, now_utc);
        }
        (!apps.is_empty()).then(|| ClosingSoon {
            apps: apps.into_iter().collect(),
            minutes,
        })
    }
}

/// A block that starts within the warning time.
struct Soon<'a> {
    /// Which block this is, so it is warned about once.
    key: String,
    at: DateTime<Utc>,
    closes: Closes<'a>,
}

enum Closes<'a> {
    /// The app rules of a list.
    List(&'a BlockList),
    /// One program, by name, at the end of its own time limit.
    Program(String),
}

fn upcoming(eng: &BlockEngine, now: DateTime<Local>) -> Vec<Soon<'_>> {
    let ahead = now + chrono::Duration::seconds(WARN_AHEAD_SECS);
    let now_utc = now.with_timezone(&Utc);
    let db = eng.db();
    // Where app use cannot be measured a shared budget does not hold apps.
    let measurable = focuser_common::session::app_usage_measurable();
    let mut soon = Vec::new();

    let lists = eng
        .block_lists()
        .iter()
        .filter(|l| l.applications.iter().any(|r| r.enabled));
    for list in lists {
        // Its hours start. A shared budget is full then, so nothing closes.
        if !(list.shared_allowance.is_some() && measurable)
            && !active_at(list, now)
            && active_at(list, ahead)
            && let Some((start, _)) = list
                .schedule
                .as_ref()
                .and_then(|s| s.active_period_at(ahead))
        {
            soon.push(Soon {
                key: format!("{}@{start}", list.id),
                at: start,
                closes: Closes::List(list),
            });
        }

        // Its shared budget runs out.
        if measurable
            && let Ok(Some(status)) = db.shared_status_at(list, now)
            && status.active
            && (1..=WARN_AHEAD_SECS).contains(&i64::from(status.remaining_secs))
            && let Some((start, _)) = occurrence(list, now)
        {
            soon.push(Soon {
                key: format!("{}@shared@{start}", list.id),
                at: now_utc + chrono::Duration::seconds(status.remaining_secs.into()),
                closes: Closes::List(list),
            });
        }
    }

    // A focus session's break ends and its list comes back on.
    if let Ok(Some(session)) = db.get_active_pomodoro_session()
        && session.current_phase.is_break()
        && !session.is_paused()
        && let Some(list) = eng
            .block_lists()
            .iter()
            .find(|l| l.id == session.block_list_id)
        && !active_at(list, now)
    {
        let at = now_utc + chrono::Duration::seconds(session.remaining_secs(now_utc).into());
        let mut on = list.clone();
        on.enabled = true;
        if at <= ahead && active_at(&on, at.with_timezone(&Local)) {
            soon.push(Soon {
                key: format!("{}@{}", session.id, session.phase_started_at),
                at,
                closes: Closes::List(list),
            });
        }
    }

    // An app's own daily limit runs out.
    for a in db.list_allowances().unwrap_or_default() {
        let AllowanceMatch::AppExecutable(exe) = &a.target else {
            continue;
        };
        if !a.enabled || eng.shared_covers_app(exe) {
            continue;
        }
        let used = db.get_allowance_used_today(a.id).unwrap_or(0);
        let left = a.daily_limit_secs.saturating_sub(used);
        if (1..=WARN_AHEAD_SECS).contains(&i64::from(left)) {
            soon.push(Soon {
                key: format!("{}@{}", a.id, now.date_naive()),
                at: now_utc + chrono::Duration::seconds(left.into()),
                closes: Closes::Program(exe.clone()),
            });
        }
    }
    soon
}

/// The open apps a block will close, as the user knows them.
fn running(
    closes: &Closes,
    procs: &[Process],
    exe_path: &mut impl FnMut(u32) -> Option<String>,
    titles: &HashMap<u32, Vec<String>>,
) -> Vec<String> {
    let procs = procs.iter().filter(|p| p.is_killable());
    let list = match closes {
        Closes::Program(exe) => {
            let open = procs.clone().any(|p| p.name.eq_ignore_ascii_case(exe));
            return open.then(|| program_name(exe)).into_iter().collect();
        }
        Closes::List(list) => list,
    };

    let rules: Vec<&AppRule> = list.applications.iter().filter(|r| r.enabled).collect();
    let wants_paths = has_rule(list, |m| matches!(m, AppMatchType::ExecutablePath(_)));
    let mut open = Vec::new();
    for p in procs {
        let path = if wants_paths { exe_path(p.pid) } else { None };
        // A browser's title follows its tab, and sites are not this.
        let own_titles = match identify_browser(&p.name) {
            Some(_) => &[][..],
            None => titles.get(&p.pid).map_or(&[][..], Vec::as_slice),
        };
        for rule in &rules {
            if rule.matches_process(&p.name, path.as_deref(), None)
                || own_titles
                    .iter()
                    .any(|t| rule.matches_process(&p.name, path.as_deref(), Some(t)))
            {
                open.push(shown(rule));
            }
        }
    }
    open
}

fn has_rule(list: &BlockList, kind: impl Fn(&AppMatchType) -> bool) -> bool {
    list.applications
        .iter()
        .any(|r| r.enabled && kind(&r.match_type))
}

/// "Minecraft" for a title rule, "Discord" for `Discord.exe`.
fn shown(rule: &AppRule) -> String {
    match &rule.match_type {
        AppMatchType::ExecutableName(v) | AppMatchType::ExecutablePath(v) => program_name(v),
        AppMatchType::WindowTitle(v) | AppMatchType::BundleId(v) => v.clone(),
    }
}

fn program_name(name: &str) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cut = file.len().saturating_sub(4);
    match (file.get(..cut), file.get(cut..)) {
        (Some(stem), Some(ext)) if !stem.is_empty() && ext.eq_ignore_ascii_case(".exe") => {
            stem.to_string()
        }
        _ => file.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use focuser_common::types::{Schedule, TimeSlot};

    const SEC: Duration = Duration::from_secs(1);

    /// Runs `close` and says what it did: (asked, terminated).
    fn step(
        closer: &mut Closer,
        blocked: &[(u32, &str)],
        at: Instant,
        windows: &[u32],
    ) -> (Vec<u32>, Vec<u32>) {
        let blocked: Vec<(u32, String)> =
            blocked.iter().map(|(p, n)| (*p, n.to_string())).collect();
        let (mut asked, mut closed) = (Vec::new(), Vec::new());
        closer.close(
            &blocked,
            at,
            |pid| {
                asked.push(pid);
                windows.contains(&pid)
            },
            |pid| {
                closed.push(pid);
                true
            },
        );
        (asked, closed)
    }

    #[test]
    fn an_app_with_a_window_gets_time_to_save() {
        let mut closer = Closer::default();
        let t = Instant::now();
        assert_eq!(
            step(&mut closer, &[(10, "Word.exe")], t, &[10]),
            (vec![10], vec![])
        );
        // Asked once only: asking again could close its "save?" dialog.
        assert_eq!(
            step(&mut closer, &[(10, "Word.exe")], t + 29 * SEC, &[10]),
            (vec![], vec![])
        );
        assert_eq!(
            step(&mut closer, &[(10, "Word.exe")], t + 30 * SEC, &[10]),
            (vec![], vec![10])
        );
    }

    #[test]
    fn an_app_with_no_window_is_closed_at_once() {
        let mut closer = Closer::default();
        assert_eq!(
            step(&mut closer, &[(10, "game.exe")], Instant::now(), &[]),
            (vec![10], vec![10])
        );
    }

    #[test]
    fn the_other_processes_of_an_app_wait_for_its_window() {
        let mut closer = Closer::default();
        let t = Instant::now();
        let code = [(10, "Code.exe"), (11, "code.exe")];
        assert_eq!(step(&mut closer, &code, t, &[10]), (vec![10, 11], vec![]));
        assert_eq!(
            step(&mut closer, &code, t + 30 * SEC, &[10]),
            (vec![], vec![10, 11])
        );
    }

    #[test]
    fn an_app_opened_again_later_is_asked_again() {
        let mut closer = Closer::default();
        let t = Instant::now();
        step(&mut closer, &[(10, "Word.exe")], t, &[10]);
        step(&mut closer, &[], t + 3 * SEC, &[]);
        assert_eq!(
            step(&mut closer, &[(12, "Word.exe")], t + 6 * SEC, &[12]),
            (vec![12], vec![])
        );
    }

    #[test]
    fn each_app_is_reported_once_when_it_is_first_stopped() {
        let mut closer = Closer::default();
        let t = Instant::now();
        let blocked = [(10, "Word.exe".to_string()), (11, "game.exe".to_string())];
        let mut first = closer.close(&blocked, t, |pid| pid == 10, |_| true);
        first.sort();
        assert_eq!(first, ["Word.exe", "game.exe"]);
        assert!(
            closer
                .close(&blocked, t + 3 * SEC, |_| true, |_| true)
                .is_empty()
        );
    }

    /// The whole way through on a real window that answers "Cancel" to the
    /// close, as someone who does not save yet would.
    #[cfg(windows)]
    #[test]
    fn a_program_that_will_not_close_is_closed_when_its_time_is_up() {
        use focuser_common::process;
        use std::os::windows::process::CommandExt;

        let mut form = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $f = New-Object System.Windows.Forms.Form; \
                 $f.Text = 'focuser-stubborn-marker'; \
                 $f.StartPosition = 'Manual'; \
                 $f.Location = New-Object System.Drawing.Point(-3000, -3000); \
                 $f.Add_FormClosing({ param($s, $e) $e.Cancel = $true }); \
                 [System.Windows.Forms.Application]::Run($f)",
            ])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .unwrap();
        let pid = form.id();
        let waited = Instant::now();
        while !process::window_titles()
            .get(&pid)
            .is_some_and(|t| t.iter().any(|t| t == "focuser-stubborn-marker"))
            && waited.elapsed() < Duration::from_secs(30)
        {
            std::thread::sleep(Duration::from_millis(100));
        }

        let blocked = [(pid, "powershell.exe".to_string())];
        let mut closer = Closer::default();
        let t = Instant::now();
        let forced = std::cell::Cell::new(0);
        let force = |pid| {
            forced.set(forced.get() + 1);
            process::terminate(pid)
        };
        closer.close(&blocked, t, process::ask_to_close, force);
        std::thread::sleep(Duration::from_secs(2));
        let alive_after_asking = form.try_wait().unwrap().is_none();
        closer.close(&blocked, t + 10 * SEC, process::ask_to_close, force);
        let forced_early = forced.get();
        closer.close(&blocked, t + 30 * SEC, process::ask_to_close, force);
        let mut exited = None;
        while exited.is_none() && t.elapsed() < Duration::from_secs(20) {
            exited = form.try_wait().unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
        if exited.is_none() {
            let _ = form.kill();
        }
        let _ = form.wait();

        assert!(alive_after_asking, "it was meant to refuse");
        assert_eq!(forced_early, 0, "closed by force before its time was up");
        assert!(exited.is_some(), "still running after its time was up");
    }

    #[test]
    fn a_program_is_named_as_the_user_knows_it() {
        assert_eq!(program_name("Discord.exe"), "Discord");
        assert_eq!(program_name(r"C:\Games\Game.EXE"), "Game");
        assert_eq!(program_name("/usr/bin/steam"), "steam");
        assert_eq!(program_name(".exe"), ".exe");
        assert_eq!(program_name("例.exe"), "例");
    }

    /// Monday 2026-09-21, local time.
    fn monday(hour: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 21, hour, min, 0).unwrap()
    }

    fn engine(list: BlockList) -> BlockEngine {
        let db = focuser_core::db::Database::open_in_memory().unwrap();
        db.create_block_list(&list).unwrap();
        BlockEngine::new(db).unwrap()
    }

    fn games(rules: Vec<AppRule>) -> BlockList {
        let mut list = BlockList::new("Games");
        list.applications = rules;
        list
    }

    fn work_hours(mut list: BlockList) -> BlockList {
        list.schedule = Some(Schedule {
            id: focuser_common::types::new_id(),
            name: "Work".into(),
            enabled: true,
            time_slots: vec![TimeSlot::new(
                chrono::Weekday::Mon,
                chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            )],
        });
        list
    }

    fn open(names: &[&str]) -> Vec<Process> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| Process {
                pid: 100 + i as u32,
                name: name.to_string(),
            })
            .collect()
    }

    fn check(
        warner: &mut Warner,
        eng: &BlockEngine,
        procs: &[Process],
        now: DateTime<Local>,
    ) -> Option<ClosingSoon> {
        warner.check(eng, procs, now, |_| None, HashMap::new)
    }

    #[test]
    fn the_user_hears_once_before_set_hours_close_an_open_app() {
        let eng = engine(work_hours(games(vec![AppRule::executable("game.exe")])));
        let mut warner = Warner::default();
        let procs = open(&["game.exe", "notepad.exe"]);

        assert_eq!(
            check(&mut warner, &eng, &procs, monday(8, 50)),
            None,
            "too early"
        );
        assert_eq!(
            check(&mut warner, &eng, &procs, monday(8, 57)),
            Some(ClosingSoon {
                apps: vec!["game".into()],
                minutes: 3
            })
        );
        assert_eq!(
            check(&mut warner, &eng, &procs, monday(8, 58)),
            None,
            "said twice"
        );
        assert_eq!(
            check(&mut warner, &eng, &procs, monday(9, 30)),
            None,
            "already blocking"
        );
    }

    #[test]
    fn nothing_open_means_no_warning_until_it_is_opened() {
        let eng = engine(work_hours(games(vec![AppRule::executable("game.exe")])));
        let mut warner = Warner::default();
        assert_eq!(
            check(&mut warner, &eng, &open(&["notepad.exe"]), monday(8, 56)),
            None
        );
        let warned = check(&mut warner, &eng, &open(&["game.exe"]), monday(8, 58));
        assert_eq!(warned.map(|w| w.minutes), Some(2));
    }

    #[test]
    fn a_list_that_is_off_warns_about_nothing() {
        let mut list = work_hours(games(vec![AppRule::executable("game.exe")]));
        list.enabled = false;
        let eng = engine(list);
        assert_eq!(
            check(
                &mut Warner::default(),
                &eng,
                &open(&["game.exe"]),
                monday(8, 57)
            ),
            None
        );
    }

    #[test]
    fn a_window_title_rule_names_what_it_matches() {
        let eng = engine(work_hours(games(vec![AppRule::window_title("Minecraft")])));
        let procs = open(&["javaw.exe", "chrome.exe"]);
        let titles = || {
            HashMap::from([
                (100, vec!["Minecraft 1.21".to_string()]),
                // A browser tab about it is the website rules' job.
                (101, vec!["Minecraft - YouTube".to_string()]),
            ])
        };
        let warned = Warner::default().check(&eng, &procs, monday(8, 57), |_| None, titles);
        assert_eq!(warned.map(|w| w.apps), Some(vec!["Minecraft".to_string()]));
    }

    #[test]
    fn the_end_of_a_focus_break_warns_about_the_apps_it_closes() {
        let mut eng = engine(games(vec![AppRule::executable("game.exe")]));
        let id = eng.block_lists()[0].id;
        let config = focuser_common::pomodoro::preset_by_key("classic").unwrap();
        focuser_core::pomodoro::start_session(&mut eng, id, config, 0).unwrap();
        let mut session = eng.db().get_active_pomodoro_session().unwrap().unwrap();
        let now = Local::now();
        // Two minutes left of the break, the list is off for it.
        session.current_phase = focuser_common::pomodoro::PomodoroPhase::ShortBreak;
        session.phase_started_at = now.with_timezone(&Utc)
            - chrono::Duration::seconds(i64::from(config.short_break_secs) - 120);
        eng.db().update_pomodoro_session(&session).unwrap();
        let mut list = eng.db().get_block_list(id).unwrap();
        list.enabled = false;
        eng.db().update_block_list(&list).unwrap();
        eng.refresh().unwrap();

        let warned = check(&mut Warner::default(), &eng, &open(&["game.exe"]), now);
        assert_eq!(
            warned,
            Some(ClosingSoon {
                apps: vec!["game".into()],
                minutes: 2
            })
        );
    }

    #[test]
    fn an_app_hears_before_its_own_time_limit_runs_out() {
        use focuser_common::allowance::Allowance;
        let db = focuser_core::db::Database::open_in_memory().unwrap();
        let a = Allowance::new(
            AllowanceMatch::AppExecutable("Steam.exe".into()),
            600,
            false,
        );
        db.create_allowance(&a).unwrap();
        db.increment_allowance_usage(a.id, 400).unwrap();
        let eng = BlockEngine::new(db).unwrap();

        let warned = check(
            &mut Warner::default(),
            &eng,
            &open(&["steam.exe"]),
            Local::now(),
        );
        assert_eq!(
            warned,
            Some(ClosingSoon {
                apps: vec!["Steam".into()],
                minutes: 4
            })
        );

        eng.db().increment_allowance_usage(a.id, 100).unwrap();
        let later = check(
            &mut Warner::default(),
            &eng,
            &open(&["steam.exe"]),
            Local::now(),
        );
        assert_eq!(later.map(|w| w.minutes), Some(2));
    }
}
