//! Detecting attempts to uninstall Focuser while a lock is active.
//!
//! A block list can be locked with `prevent_uninstall`. Honouring that means
//! noticing when an uninstaller, a package manager or a shell is removing
//! *Focuser specifically* and closing it before it finishes.
//!
//! The targeting check is the whole safety story. Uninstallers are shared
//! binaries — `msiexec.exe` removes any MSI, `apt` removes any package — and
//! shells run everything, so a loose match has Focuser killing work the user
//! deliberately started. It used to flag any shell whose command line merely
//! contained "focuser" and a word like "uninstall" anywhere, which killed
//! developer scripts that read `prevent_uninstall` out of the database.
//!
//! So a command line is now read the way a shell would read it: split into
//! simple commands (respecting quotes, comments and heredoc bodies), and a
//! command only counts when its *program* removes things, it carries the
//! removal verb that program needs, and its *arguments* name a Focuser
//! package, unit or install path. Which programs, verbs and paths those are is
//! per OS and lives in `imp`.

use crate::process::Process;

/// What a removal command has to name before it is aimed at Focuser.
#[derive(Debug, Clone, Copy)]
enum Target {
    /// A package or app id, e.g. `focuser`, `focuser:amd64`, `com.focuser.app`.
    Package,
    /// A file or directory Focuser installs or keeps its data in.
    Path,
    /// The systemd user unit.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Unit,
}

/// One way of removing Focuser: `program verb ... target`.
struct Rule {
    /// Program names, lower case, without `.exe`.
    programs: &'static [&'static str],
    /// At least one must appear among the arguments; empty means the program
    /// only ever removes (`rm`). A trailing `*` matches by prefix (`-R*`).
    verbs: &'static [&'static str],
    target: Target,
}

#[cfg(windows)]
mod imp {
    use super::{Rule, Target};

    /// Dedicated uninstallers. Naming Focuser anywhere is enough for these:
    /// their only job is removing software, and an MSI is addressed by a
    /// GUID or package path, not a verb.
    pub const UNINSTALLERS: &[&str] = &["msiexec", "unins000", "uninstall", "uninst", "au_"];

    /// Interpreters worth inspecting: a scripted uninstall hides behind a
    /// generic name, so the command line is the only thing that gives it away.
    pub const SHELLS: &[&str] = &["powershell", "pwsh", "cmd"];

    pub const RULES: &[Rule] = &[
        Rule {
            programs: &["winget"],
            verbs: &["uninstall", "remove", "rm"],
            target: Target::Package,
        },
        Rule {
            programs: &["choco", "scoop"],
            verbs: &["uninstall"],
            target: Target::Package,
        },
        Rule {
            programs: &["uninstall-package"],
            verbs: &[],
            target: Target::Package,
        },
        Rule {
            programs: &["remove-item", "ri", "rm", "del", "erase", "rd", "rmdir"],
            verbs: &[],
            target: Target::Path,
        },
    ];

    /// Where Focuser is installed and keeps its data, lower case with `/`
    /// separators.
    pub const PATH_MARKERS: &[&str] = &[
        "program files/focuser",
        "program files (x86)/focuser",
        "%programfiles%/focuser",
        "$env:programfiles/focuser",
        "appdata/local/focuser",
        "appdata/roaming/focuser",
        "%localappdata%/focuser",
        "%appdata%/focuser",
        "$env:localappdata/focuser",
        "$env:appdata/focuser",
        "com.focuser.",
    ];
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Rule, Target};

    /// macOS has no uninstaller convention, so these are the third-party app
    /// removers people actually use. Their only job is removing apps.
    pub const UNINSTALLERS: &[&str] = &[
        "appcleaner",
        "cleanmymac",
        "appzapper",
        "appdelete",
        "trashme",
        "pearcleaner",
    ];

    pub const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "pwsh"];

    pub const RULES: &[Rule] = &[
        Rule {
            programs: &["brew"],
            verbs: &["uninstall", "remove", "rm"],
            target: Target::Package,
        },
        Rule {
            programs: &["rm", "unlink", "srm", "rmdir", "trash"],
            verbs: &[],
            target: Target::Path,
        },
        Rule {
            programs: &["launchctl"],
            verbs: &["unload", "bootout", "remove", "disable"],
            target: Target::Path,
        },
    ];

    pub const PATH_MARKERS: &[&str] = &[
        "/applications/focuser.app",
        "/library/launchagents/focuser",
        "com.focuser.",
    ];
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Rule, Target};

    /// Every Linux remover is a general tool, so all of them go through
    /// [`RULES`] instead.
    pub const UNINSTALLERS: &[&str] = &[];

    pub const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "pwsh"];

    pub const RULES: &[Rule] = &[
        Rule {
            programs: &["apt", "apt-get", "aptitude"],
            verbs: &["remove", "purge", "autoremove", "autopurge"],
            target: Target::Package,
        },
        Rule {
            programs: &["dpkg"],
            verbs: &["-r", "-P", "--remove", "--purge"],
            target: Target::Package,
        },
        Rule {
            programs: &["dnf", "yum"],
            verbs: &["remove", "erase", "autoremove"],
            target: Target::Package,
        },
        Rule {
            programs: &["rpm"],
            verbs: &["-e*", "--erase"],
            target: Target::Package,
        },
        Rule {
            programs: &["pacman"],
            verbs: &["-R*", "--remove"],
            target: Target::Package,
        },
        Rule {
            programs: &["zypper"],
            verbs: &["remove", "rm"],
            target: Target::Package,
        },
        Rule {
            programs: &["snap"],
            verbs: &["remove"],
            target: Target::Package,
        },
        Rule {
            programs: &["flatpak"],
            verbs: &["uninstall", "remove"],
            target: Target::Package,
        },
        Rule {
            programs: &["rm", "unlink", "shred", "rmdir", "trash", "trash-put"],
            verbs: &[],
            target: Target::Path,
        },
        Rule {
            programs: &["gio"],
            verbs: &["trash", "remove"],
            target: Target::Path,
        },
        Rule {
            programs: &["systemctl"],
            verbs: &["disable", "mask"],
            target: Target::Unit,
        },
    ];

    /// What the .deb installs, plus the data, config and autostart entries.
    pub const PATH_MARKERS: &[&str] = &[
        "/usr/bin/focuser",
        "/usr/local/bin/focuser",
        "/usr/lib/focuser",
        "/opt/focuser",
        "/usr/share/applications/focuser",
        "/systemd/user/focuser.service",
        "/.local/share/focuser",
        "/.config/focuser",
        "/autostart/focuser",
        "com.focuser.",
    ];
}

/// Programs that only run whatever follows them.
const WRAPPERS: &[&str] = &[
    "sudo", "doas", "pkexec", "env", "nohup", "exec", "command", "time", "nice", "ionice", "stdbuf",
];

/// How deep `bash -c "eval '...'"` nesting is followed.
const MAX_DEPTH: u8 = 4;

/// Whether a command line names Focuser at all.
fn targets_focuser(cmdline: &str) -> bool {
    cmdline.to_lowercase().contains("focuser")
}

/// The bare program name: lower case, no directory, no `.exe`, and no `-`
/// that a login shell puts in front of its own name.
fn program_name(word: &str) -> String {
    let lower = word.to_lowercase();
    let base = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    let base = base.strip_suffix(".exe").unwrap_or(base);
    base.trim_start_matches('-').to_string()
}

fn is_dedicated_uninstaller(name: &str) -> bool {
    imp::UNINSTALLERS.contains(&program_name(name).as_str())
}

fn is_shell(program: &str) -> bool {
    imp::SHELLS.contains(&program)
}

/// Whether a process is worth reading the command line of.
fn is_candidate(name: &str) -> bool {
    let program = program_name(name);
    is_dedicated_uninstaller(name)
        || is_shell(&program)
        || imp::RULES
            .iter()
            .any(|r| r.programs.contains(&program.as_str()))
}

/// Split a command line into simple commands, each a list of words, the way
/// a POSIX shell would — closely enough to tell which word is a program.
///
/// Quoted text stays one word, so `echo "apt remove focuser"` is an `echo`.
/// Heredoc bodies and comments are dropped: they are data, not commands, and
/// a Python script fed through `<<EOF` can say whatever it likes.
fn commands(script: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = script.chars().collect();
    let len = chars.len();
    let mut out = Vec::new();
    let mut cmd: Vec<String> = Vec::new();
    let mut word: Option<String> = None;
    let mut heredocs: Vec<String> = Vec::new();

    fn end_word(word: &mut Option<String>, cmd: &mut Vec<String>) {
        if let Some(w) = word.take() {
            cmd.push(w);
        }
    }
    fn end_cmd(word: &mut Option<String>, cmd: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        end_word(word, cmd);
        if !cmd.is_empty() {
            out.push(std::mem::take(cmd));
        }
    }

    let mut i = 0;
    while i < len {
        let c = chars[i];
        match c {
            '\'' => {
                let w = word.get_or_insert_with(String::new);
                i += 1;
                while i < len && chars[i] != '\'' {
                    w.push(chars[i]);
                    i += 1;
                }
            }
            '"' => {
                let w = word.get_or_insert_with(String::new);
                i += 1;
                while i < len && chars[i] != '"' {
                    // Inside double quotes a backslash only escapes these;
                    // anywhere else it is literal, which keeps `C:\Program
                    // Files` intact.
                    if chars[i] == '\\'
                        && i + 1 < len
                        && matches!(chars[i + 1], '"' | '\\' | '$' | '`')
                    {
                        i += 1;
                    }
                    w.push(chars[i]);
                    i += 1;
                }
            }
            '\\' => {
                let w = word.get_or_insert_with(String::new);
                match chars.get(i + 1) {
                    Some('\n') => i += 1,
                    Some(&n) if n.is_whitespace() || matches!(n, '\'' | '"' | '\\') => {
                        w.push(n);
                        i += 1;
                    }
                    _ => w.push('\\'),
                }
            }
            '#' if word.is_none() => {
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '\n' => {
                end_cmd(&mut word, &mut cmd, &mut out);
                i += 1;
                for delim in heredocs.drain(..) {
                    while i < len {
                        let start = i;
                        while i < len && chars[i] != '\n' {
                            i += 1;
                        }
                        let line: String = chars[start..i].iter().collect();
                        i += 1;
                        if line.trim() == delim {
                            break;
                        }
                    }
                }
                continue;
            }
            '<' if chars.get(i + 1) == Some(&'<') && chars.get(i + 2) != Some(&'<') => {
                end_word(&mut word, &mut cmd);
                i += 2;
                if chars.get(i) == Some(&'-') {
                    i += 1;
                }
                while i < len && matches!(chars[i], ' ' | '\t') {
                    i += 1;
                }
                let mut delim = String::new();
                while i < len
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], ';' | '&' | '|' | '<' | '>' | '(' | ')')
                {
                    if !matches!(chars[i], '\'' | '"' | '\\') {
                        delim.push(chars[i]);
                    }
                    i += 1;
                }
                if !delim.is_empty() {
                    heredocs.push(delim);
                }
                continue;
            }
            ';' | '&' | '|' | '(' | ')' | '`' => end_cmd(&mut word, &mut cmd, &mut out),
            '<' | '>' => end_word(&mut word, &mut cmd),
            c if c.is_whitespace() => end_word(&mut word, &mut cmd),
            c => word.get_or_insert_with(String::new).push(c),
        }
        i += 1;
    }
    end_cmd(&mut word, &mut cmd, &mut out);
    out
}

/// A switch rather than an operand: `-c`, `--user`, and on Windows `/c`.
fn is_flag(word: &str) -> bool {
    word.starts_with('-') || (word.starts_with('/') && word.len() <= 3)
}

/// The switch a shell takes its script from: `-c` (also `-lc`), PowerShell's
/// `-Command`, cmd's `/c` and `/k`.
fn is_script_flag(word: &str) -> bool {
    let lower = word.to_lowercase();
    matches!(lower.as_str(), "-command" | "/c" | "/k")
        || (lower.starts_with('-') && !lower.starts_with("--") && lower.ends_with('c'))
}

/// Drop `sudo -E`, `env FOO=1` and the like from the front of a command.
fn strip_wrappers(mut words: &[String]) -> &[String] {
    loop {
        match words.split_first() {
            Some((first, rest)) if WRAPPERS.contains(&program_name(first).as_str()) => {
                words = rest;
                while words.first().is_some_and(|w| is_flag(w)) {
                    words = &words[1..];
                }
            }
            Some((first, rest))
                if first.contains('=')
                    && first.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') =>
            {
                words = rest;
            }
            _ => return words,
        }
    }
}

fn verb_matches(arg: &str, verb: &str) -> bool {
    if let Some(prefix) = verb.strip_suffix('*') {
        arg.starts_with(prefix)
    } else if verb.starts_with('-') {
        arg == verb
    } else {
        arg.eq_ignore_ascii_case(verb)
    }
}

impl Target {
    fn named_in(self, args: &[String]) -> bool {
        match self {
            Target::Package => args.iter().any(|a| {
                a.to_lowercase()
                    .split(['.', ':', '=', '@', '/'])
                    .any(|seg| seg.starts_with("focuser"))
            }),
            // Joined, because a path with a space in it may have lost its
            // quotes on the way here.
            Target::Path => {
                let joined = args.join(" ").to_lowercase().replace('\\', "/");
                imp::PATH_MARKERS.iter().any(|m| joined.contains(m))
            }
            Target::Unit => args.iter().any(|a| {
                let lower = a.to_lowercase();
                let unit = lower.rsplit('/').next().unwrap_or(&lower);
                unit == "focuser" || unit == "focuser.service"
            }),
        }
    }
}

impl Rule {
    fn matches(&self, program: &str, args: &[String]) -> bool {
        self.programs.contains(&program)
            && (self.verbs.is_empty()
                || args
                    .iter()
                    .any(|a| self.verbs.iter().any(|v| verb_matches(a, v))))
            && self.target.named_in(args)
    }
}

fn script_removes_focuser(script: &str, depth: u8) -> bool {
    depth <= MAX_DEPTH
        && commands(script)
            .iter()
            .any(|words| command_removes_focuser(words, depth))
}

fn command_removes_focuser(words: &[String], depth: u8) -> bool {
    let Some((first, args)) = strip_wrappers(words).split_first() else {
        return false;
    };
    let program = program_name(first);
    if program == "eval" || is_shell(&program) {
        // The script a shell runs arrives either as the argument after its
        // `-c`, or, where the OS joined argv with spaces, as everything after
        // the flags. Any other quoted argument is data for some command.
        let rest = args.iter().skip_while(|a| is_flag(a)).cloned();
        let joined = rest.collect::<Vec<_>>().join(" ");
        return args
            .windows(2)
            .filter(|pair| is_script_flag(&pair[0]))
            .map(|pair| &pair[1])
            .chain(std::iter::once(&joined))
            .any(|s| script_removes_focuser(s, depth + 1));
    }
    imp::RULES.iter().any(|rule| rule.matches(&program, args))
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
    if is_dedicated_uninstaller(name) {
        return true;
    }
    is_candidate(name) && script_removes_focuser(cmdline, 0)
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
        .filter(|p| p.is_killable() && is_candidate(&p.name))
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

    fn a_shell() -> &'static str {
        imp::SHELLS[0]
    }

    /// A shell command line that really does remove Focuser on this OS.
    fn a_removal() -> &'static str {
        if cfg!(windows) {
            r#"cmd.exe /c rd /s /q "%LOCALAPPDATA%\Focuser""#
        } else if cfg!(target_os = "macos") {
            "sh -c rm -rf /Applications/Focuser.app"
        } else {
            "sh -c rm -rf /usr/bin/focuser-ui"
        }
    }

    #[test]
    fn commands_split_on_separators_but_not_inside_quotes() {
        let cmds = commands("echo 'a; b' && ls -l | grep \"x y\"; true");
        assert_eq!(
            cmds,
            [
                vec!["echo".to_string(), "a; b".into()],
                vec!["ls".into(), "-l".into()],
                vec!["grep".into(), "x y".into()],
                vec!["true".into()],
            ]
        );
    }

    #[test]
    fn commands_skip_heredoc_bodies_and_comments() {
        let cmds = commands(
            "cat > x.sh <<'EOF'\nrm -rf /usr/bin/focuser-ui\nEOF\n# rm -rf /opt/focuser\necho done",
        );
        assert_eq!(
            cmds,
            [
                vec!["cat".to_string(), "x.sh".into()],
                vec!["echo".into(), "done".into()],
            ]
        );
    }

    #[test]
    fn a_shell_removing_focuser_is_an_attempt() {
        assert!(is_uninstall_attempt(a_shell(), Some(a_removal())));
    }

    #[test]
    fn a_shell_that_only_mentions_focuser_and_removal_is_left_alone() {
        // The bug this module was rewritten for: the words are there, but
        // nothing is removing Focuser.
        for line in [
            "cat focuser.log",
            "grep -rn prevent_uninstall crates/focuser-ui",
            "git commit -m \"Remove the focuser uninstall bug\"",
            "sqlite3 focuser.db \"DELETE FROM block_lists WHERE prevent_uninstall = 1\"",
        ] {
            assert!(!is_uninstall_attempt(a_shell(), Some(line)), "{line}");
        }
    }

    #[test]
    fn an_unreadable_command_line_is_never_an_attempt() {
        assert!(!is_uninstall_attempt(a_shell(), None));
    }

    #[test]
    fn an_ordinary_program_is_never_an_attempt() {
        assert!(!is_uninstall_attempt("notepad.exe", Some(a_removal())));
    }

    #[test]
    fn detect_returns_only_the_offending_pids() {
        let processes = vec![
            proc(1000, a_shell()),
            proc(1001, a_shell()),
            proc(1002, "notepad.exe"),
        ];
        let found = detect(&processes, |pid| match pid {
            1000 => Some(a_removal().into()),
            1001 => Some("echo prevent_uninstall focuser".into()),
            _ => None,
        });
        assert_eq!(found, vec![1000]);
    }

    #[test]
    fn detect_never_reads_the_command_line_of_an_ordinary_process() {
        // Reading a command line shells out on macOS. Doing it for every
        // process would make the blocking loop crawl.
        let processes = vec![proc(1000, "notepad.exe"), proc(1001, a_shell())];
        let mut asked = Vec::new();
        detect(&processes, |pid| {
            asked.push(pid);
            None
        });
        assert_eq!(asked, vec![1001]);
    }

    #[test]
    fn our_own_process_is_never_flagged() {
        let me = proc(std::process::id(), a_shell());
        assert!(detect(&[me], |_| Some(a_removal().into())).is_empty());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn a_dedicated_uninstaller_only_needs_to_name_focuser() {
        let uninstaller = imp::UNINSTALLERS[0];
        assert!(is_uninstall_attempt(
            uninstaller,
            Some("... /x {GUID} Focuser")
        ));
        assert!(is_uninstall_attempt(
            &uninstaller.to_uppercase(),
            Some("... FOCUSER")
        ));
        // Uninstalling unrelated software must not be interrupted.
        assert!(!is_uninstall_attempt(
            uninstaller,
            Some("... /x {GUID} SomeOtherApp")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_lines() {
        let yes = [
            ("winget.exe", "winget uninstall Focuser"),
            ("winget.exe", "winget uninstall --id Focuser.Focuser"),
            (
                "powershell.exe",
                r#""C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -Command "Remove-Item -Recurse -Force 'C:\Program Files\Focuser'""#,
            ),
            ("pwsh.exe", r#"pwsh -c "Uninstall-Package -Name Focuser""#),
            (
                "cmd.exe",
                r#"C:\Windows\system32\cmd.exe /c del /q "%APPDATA%\focuser\Focuser\data\focuser.db""#,
            ),
        ];
        for (name, line) in yes {
            assert!(is_uninstall_attempt(name, Some(line)), "{line}");
        }
        let no = [
            ("winget.exe", "winget install Focuser"),
            ("winget.exe", "winget uninstall SomeOtherApp"),
            (
                "powershell.exe",
                r#"powershell -Command "Get-Content $env:APPDATA\focuser\Focuser\data\focuser.log | Select-String uninstall""#,
            ),
            (
                "cmd.exe",
                r#"cmd /c rd /s /q C:\code\Focuser\target && echo remove"#,
            ),
        ];
        for (name, line) in no {
            assert!(!is_uninstall_attempt(name, Some(line)), "{line}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_command_lines() {
        let yes = [
            ("brew", "brew uninstall --cask focuser"),
            ("rm", "rm -rf /Applications/Focuser.app"),
            (
                "zsh",
                "zsh -c rm -rf \"$HOME/Library/Application Support/com.focuser.Focuser\"",
            ),
            (
                "launchctl",
                "launchctl unload /Users/u/Library/LaunchAgents/Focuser.plist",
            ),
        ];
        for (name, line) in yes {
            assert!(is_uninstall_attempt(name, Some(line)), "{line}");
        }
        let no = [
            ("brew", "brew install --cask focuser"),
            ("rm", "rm -rf /Users/u/code/Focuser/target"),
            ("zsh", "zsh -c grep prevent_uninstall focuser.log"),
        ];
        for (name, line) in no {
            assert!(!is_uninstall_attempt(name, Some(line)), "{line}");
        }
    }

    /// Linux joins argv with spaces, so `bash -c "<script>"` arrives as
    /// `bash -c <script>`; these are written the way `/proc` reports them.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_removals_are_attempts() {
        let yes = [
            ("apt", "apt remove focuser"),
            ("apt-get", "/usr/bin/apt-get purge -y focuser"),
            ("apt", "apt autoremove --purge focuser:amd64"),
            ("dpkg", "dpkg -P focuser"),
            ("dpkg", "dpkg --remove focuser"),
            ("dnf", "dnf remove focuser"),
            ("rpm", "rpm -e focuser"),
            ("pacman", "pacman -Rns focuser"),
            ("snap", "snap remove focuser"),
            ("flatpak", "flatpak uninstall --user com.focuser.app"),
            ("rm", "rm -f /usr/bin/focuser-ui"),
            ("unlink", "unlink /usr/bin/focuser-cli"),
            ("rm", "rm -rf /home/u/.local/share/focuser"),
            ("rm", "rm -rf /home/u/.config/autostart/Focuser.desktop"),
            (
                "systemctl",
                "systemctl --user disable --now focuser.service",
            ),
            ("systemctl", "systemctl --user mask focuser"),
            ("bash", "bash -c sudo apt remove focuser"),
            ("sh", "sh -c echo bye; rm -rf ~/.local/share/focuser"),
            ("bash", "bash -c cd /tmp && sudo -E dpkg -r focuser"),
            (
                "bash",
                "/bin/bash -c eval 'sudo apt-get remove focuser' < /dev/null",
            ),
            ("zsh", "zsh -c systemctl --user disable focuser.service"),
        ];
        for (name, line) in yes {
            assert!(is_uninstall_attempt(name, Some(line)), "{name}: {line}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_commands_that_merely_mention_focuser_are_not_attempts() {
        let no = [
            ("apt", "apt install focuser"),
            ("apt", "apt remove vim"),
            ("dpkg", "dpkg -i focuser_0.9.1_amd64.deb"),
            ("systemctl", "systemctl --user status focuser.service"),
            ("systemctl", "systemctl --user restart focuser"),
            ("rm", "rm -rf /home/u/code/Focuser/target"),
            ("rm", "rm /tmp/focuser-test.log"),
            // What actually got killed on 2026-10-03: a python heredoc that
            // edits the database, and a pipeline grepping the journal.
            (
                "bash",
                "bash -c python3 - <<'EOF'\nimport sqlite3\ndb = sqlite3.connect('/home/u/.local/share/focuser/focuser.db')\nprint(db.execute(\"SELECT id, prevent_uninstall FROM block_lists\").fetchall())\ndb.execute(\"DELETE FROM block_lists WHERE prevent_uninstall = 1\")\ndb.commit()\nEOF",
            ),
            (
                "bash",
                "/bin/bash -c -l source /home/u/.claude/shell-snapshots/snapshot-bash.sh && shopt -u extglob 2>/dev/null || true && eval 'python3 - <<'\\''EOF'\\''\nimport subprocess\n# uninstall check: remove nothing\nsubprocess.run([\"rm\", \"-rf\", \"/usr/bin/focuser-ui\"])\nprint(\"prevent_uninstall\")\nEOF' < /dev/null && pwd -P >| /tmp/claude-cwd",
            ),
            (
                "bash",
                "bash -c journalctl --user -u focuser.service | grep -E 'prevent_uninstall|remove|DELETE FROM'",
            ),
            (
                "sh",
                "sh -c sqlite3 ~/.local/share/focuser/focuser.db \"DELETE FROM blocks\"; echo uninstall",
            ),
            (
                "bash",
                "bash -c cat > /tmp/x.sh <<EOF\napt remove focuser\nrm -rf /usr/bin/focuser-ui\nEOF\nchmod +x /tmp/x.sh",
            ),
            ("bash", "bash -c echo \"apt remove focuser\""),
        ];
        for (name, line) in no {
            assert!(!is_uninstall_attempt(name, Some(line)), "{name}: {line}");
        }
    }
}
