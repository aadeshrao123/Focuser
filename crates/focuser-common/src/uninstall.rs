//! Detecting attempts to uninstall Focuser while a lock is active.
//!
//! A block list can be locked with `prevent_uninstall`. Honouring that means
//! noticing when an uninstaller or a package manager is running *against
//! Focuser specifically* and closing it before it finishes.
//!
//! The targeting check is the whole safety story. Uninstallers are shared
//! binaries — `msiexec.exe` removes any MSI, `apt` removes any package — so
//! matching on the process name alone would have Focuser killing unrelated
//! software the user deliberately started. Nothing is flagged unless its
//! command line names Focuser.

use crate::process::Process;

/// Programs that remove software. Shared binaries, hence the targeting check.
#[cfg(windows)]
const UNINSTALLERS: &[&str] = &[
    "msiexec.exe",
    "unins000.exe",
    "uninstall.exe",
    "uninst.exe",
    "au_.exe",
];

/// macOS has no uninstaller convention, so these are the third-party app
/// removers people actually use.
#[cfg(target_os = "macos")]
const UNINSTALLERS: &[&str] = &[
    "AppCleaner",
    "CleanMyMac",
    "AppZapper",
    "AppDelete",
    "TrashMe",
    "Pearcleaner",
];

#[cfg(target_os = "linux")]
const UNINSTALLERS: &[&str] = &[
    "apt", "apt-get", "dpkg", "dnf", "yum", "rpm", "pacman", "zypper", "snap", "flatpak",
];

/// Interpreters worth inspecting: a scripted uninstall hides behind a generic
/// name, so the command line is the only thing that gives it away.
#[cfg(windows)]
const SHELLS: &[&str] = &["powershell.exe", "pwsh.exe", "cmd.exe"];

#[cfg(unix)]
const SHELLS: &[&str] = &["sh", "bash", "zsh", "pwsh"];

/// Words that mean "take this away".
const REMOVAL_VERBS: &[&str] = &["uninstall", "remove", "purge", "erase", "--delete"];

/// Whether a command line is aimed at Focuser rather than something else.
pub fn targets_focuser(cmdline: &str) -> bool {
    cmdline.to_lowercase().contains("focuser")
}

/// Whether this process removes software, by name alone.
fn is_uninstaller(name: &str) -> bool {
    UNINSTALLERS
        .iter()
        .any(|u| u.eq_ignore_ascii_case(name.trim()))
}

fn is_shell(name: &str) -> bool {
    SHELLS.iter().any(|s| s.eq_ignore_ascii_case(name.trim()))
}

fn mentions_removal(cmdline: &str) -> bool {
    let lower = cmdline.to_lowercase();
    REMOVAL_VERBS.iter().any(|verb| lower.contains(verb))
}

/// Decide whether one process is an uninstall attempt against Focuser.
///
/// Split out from [`detect`] so the rules can be tested without a real process
/// table: `cmdline` is whatever the OS reported for that pid, if anything.
pub fn is_uninstall_attempt(name: &str, cmdline: Option<&str>) -> bool {
    let Some(cmdline) = cmdline else {
        // No command line means no way to tell who the target is, and a
        // guess here would kill an innocent uninstaller.
        return false;
    };
    if !targets_focuser(cmdline) {
        return false;
    }
    // A dedicated uninstaller naming Focuser is enough on its own. A shell is
    // only suspicious once it also says what it intends to do.
    is_uninstaller(name) || (is_shell(name) && mentions_removal(cmdline))
}

/// Pids that appear to be uninstalling Focuser.
///
/// `read_cmdline` is injected so the caller supplies the real reader in
/// production and a fixture in tests. It is only consulted for processes whose
/// name is already a candidate, because reading a command line is expensive.
pub fn detect(
    processes: &[Process],
    mut read_cmdline: impl FnMut(u32) -> Option<String>,
) -> Vec<u32> {
    processes
        .iter()
        .filter(|p| p.is_killable() && (is_uninstaller(&p.name) || is_shell(&p.name)))
        .filter(|p| is_uninstall_attempt(&p.name, read_cmdline(p.pid).as_deref()))
        .map(|p| p.pid)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str) -> Process {
        Process {
            pid,
            name: name.into(),
        }
    }

    /// The name of a real uninstaller on whichever OS is running the test.
    fn an_uninstaller() -> &'static str {
        UNINSTALLERS[0]
    }

    fn a_shell() -> &'static str {
        SHELLS[0]
    }

    #[test]
    fn an_uninstaller_aimed_at_focuser_is_an_attempt() {
        assert!(is_uninstall_attempt(
            an_uninstaller(),
            Some("... /x {GUID} Focuser")
        ));
    }

    #[test]
    fn the_same_uninstaller_aimed_elsewhere_is_left_alone() {
        // The point of the targeting check: uninstalling unrelated software
        // must not be interrupted.
        assert!(!is_uninstall_attempt(
            an_uninstaller(),
            Some("... /x {GUID} SomeOtherApp")
        ));
    }

    #[test]
    fn matching_is_case_insensitive_on_both_sides() {
        let shouty = an_uninstaller().to_uppercase();
        assert!(is_uninstall_attempt(&shouty, Some("remove FOCUSER now")));
    }

    #[test]
    fn a_shell_needs_both_focuser_and_a_removal_verb() {
        assert!(is_uninstall_attempt(
            a_shell(),
            Some("uninstall focuser silently")
        ));
        // Mentions Focuser but is not removing it — e.g. tailing a log.
        assert!(!is_uninstall_attempt(a_shell(), Some("cat focuser.log")));
        // Removing something, but not us.
        assert!(!is_uninstall_attempt(
            a_shell(),
            Some("remove some-package")
        ));
    }

    #[test]
    fn an_unreadable_command_line_is_never_an_attempt() {
        // Guessing from the name alone would kill any uninstaller running for
        // any reason, so an unknown command line has to mean "leave it".
        assert!(!is_uninstall_attempt(an_uninstaller(), None));
    }

    #[test]
    fn an_ordinary_program_is_never_an_attempt() {
        assert!(!is_uninstall_attempt(
            "notepad.exe",
            Some("notepad.exe uninstall focuser")
        ));
    }

    #[test]
    fn detect_returns_only_the_offending_pids() {
        let processes = vec![
            proc(1000, an_uninstaller()),
            proc(1001, an_uninstaller()),
            proc(1002, "notepad.exe"),
        ];
        let found = detect(&processes, |pid| match pid {
            1000 => Some("removing Focuser".into()),
            1001 => Some("removing SomethingElse".into()),
            _ => None,
        });
        assert_eq!(found, vec![1000]);
    }

    #[test]
    fn detect_never_reads_the_command_line_of_an_ordinary_process() {
        // Reading a command line shells out on most platforms. Doing it for
        // every process would make the blocking loop crawl.
        let processes = vec![proc(1000, "notepad.exe"), proc(1001, an_uninstaller())];
        let mut asked = Vec::new();
        detect(&processes, |pid| {
            asked.push(pid);
            None
        });
        assert_eq!(asked, vec![1001]);
    }

    #[test]
    fn our_own_process_is_never_flagged() {
        let me = proc(std::process::id(), an_uninstaller());
        assert!(detect(&[me], |_| Some("uninstall focuser".into())).is_empty());
    }
}
