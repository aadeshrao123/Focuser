//! End-to-end tests that run the real `focuser` binary against a scratch database.
//!
//! These are the tests that replace manual release checking. They exercise the
//! same `execute()` the GUI calls, so a scenario passing here is evidence about
//! the shipped application, not about a mock.
//!
//! `CARGO_BIN_EXE_focuser-cli` is set by Cargo for integration tests, so the
//! binary under test is always the one just built.

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// A CLI invocation against an isolated database.
struct Cli {
    _dir: TempDir,
    db: std::path::PathBuf,
}

impl Cli {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let db = dir.path().join("test.db");
        Self { _dir: dir, db }
    }

    fn raw(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_focuser-cli"))
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .expect("failed to run focuser-cli")
    }

    /// Run, expecting success. Returns trimmed stdout.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.raw(args);
        assert!(
            out.status.success(),
            "expected `{}` to succeed, got exit {:?}\nstderr: {}",
            args.join(" "),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Run, expecting failure. Returns (exit code, trimmed stderr).
    fn fail(&self, args: &[&str]) -> (i32, String) {
        let out = self.raw(args);
        assert!(
            !out.status.success(),
            "expected `{}` to fail, but it succeeded",
            args.join(" ")
        );
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        )
    }

    /// Run with `--json` and parse the result.
    fn json(&self, args: &[&str]) -> serde_json::Value {
        let mut with_json = vec!["--json"];
        with_json.extend_from_slice(args);
        serde_json::from_str(&self.ok(&with_json)).expect("stdout was not valid JSON")
    }

    /// Create a list and return its id.
    fn make_list(&self, name: &str) -> String {
        self.json(&["list", "create", name])["data"]["id"]
            .as_str()
            .expect("created list should carry an id")
            .to_string()
    }
}

#[test]
fn creating_a_list_makes_it_appear_in_ls() {
    let cli = Cli::new();
    let id = cli.make_list("Social");

    let lists = cli.json(&["list", "ls"]);
    let data = lists["data"].as_array().unwrap();

    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["id"], id);
    assert_eq!(data[0]["name"], "Social");
    assert_eq!(data[0]["enabled"], true);
}

#[test]
fn json_output_is_machine_readable_and_human_output_is_not_required_to_be() {
    let cli = Cli::new();
    cli.make_list("Social");

    // The contract scripts rely on: `--json` parses, and carries a discriminant.
    let result = cli.json(&["list", "ls"]);
    assert_eq!(result["kind"], "block_lists");

    // Human output must at least mention the name; its exact shape is not a
    // stable interface and is deliberately not asserted further.
    assert!(cli.ok(&["list", "ls"]).contains("Social"));
}

#[test]
fn exit_codes_distinguish_failure_kinds() {
    let cli = Cli::new();
    let missing = "00000000-0000-0000-0000-000000000000";

    // Validation — the caller sent something invalid.
    let (code, stderr) = cli.fail(&["list", "create", "   "]);
    assert_eq!(code, 2, "validation should exit 2");
    assert!(stderr.starts_with("validation:"), "got: {stderr}");

    // Not found — the caller referenced something that does not exist.
    let (code, stderr) = cli.fail(&["list", "rm", missing]);
    assert_eq!(code, 4, "not-found should exit 4");
    assert!(stderr.starts_with("block_list_not_found:"), "got: {stderr}");
}

#[test]
fn a_missing_rule_is_reported_rather_than_silently_succeeding() {
    let cli = Cli::new();
    let list = cli.make_list("Sites");
    let missing = "00000000-0000-0000-0000-000000000000";

    // The pre-rewrite command used Vec::retain and returned success here.
    let (code, stderr) = cli.fail(&["site", "rm", &list, missing]);
    assert_eq!(code, 4);
    assert!(stderr.contains("rule_not_found"), "got: {stderr}");
}

#[test]
fn full_website_rule_lifecycle() {
    let cli = Cli::new();
    let list = cli.make_list("Sites");

    let rule = cli.json(&["site", "add", &list, "example.com"])["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let after_add = cli.json(&["list", "ls"]);
    assert_eq!(
        after_add["data"][0]["websites"].as_array().unwrap().len(),
        1
    );

    cli.ok(&["site", "rm", &list, &rule]);

    let after_rm = cli.json(&["list", "ls"]);
    assert!(
        after_rm["data"][0]["websites"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn bulk_import_reports_how_many_values_it_accepted() {
    let cli = Cli::new();
    let list = cli.make_list("Sites");

    let added = cli.json(&[
        "site",
        "import",
        &list,
        "example.com",
        "EXAMPLE.com", // duplicate once normalised
        "# comment",
        "",
        "other.com",
    ]);

    assert_eq!(added["kind"], "count");
    assert_eq!(added["data"], 2);
}

#[test]
fn bulk_import_reads_a_file() {
    let cli = Cli::new();
    let list = cli.make_list("Sites");

    let path = cli.db.parent().unwrap().join("domains.txt");
    std::fs::write(&path, "one.com\n# a comment\n\ntwo.com\n").unwrap();

    let added = cli.json(&["site", "import", &list, "--file", path.to_str().unwrap()]);
    assert_eq!(added["data"], 2);
}

#[test]
fn toggling_a_list_persists_across_invocations() {
    let cli = Cli::new();
    let id = cli.make_list("Social");

    cli.ok(&["list", "disable", &id]);
    assert_eq!(cli.json(&["list", "ls"])["data"][0]["enabled"], false);

    cli.ok(&["list", "enable", &id]);
    assert_eq!(cli.json(&["list", "ls"])["data"][0]["enabled"], true);
}

#[test]
fn protection_prevents_modification_and_is_reported_by_status() {
    let cli = Cli::new();
    let id = cli.make_list("Committed");

    cli.ok(&["protect", "enable", &id, "--minutes", "60"]);

    let (code, stderr) = cli.fail(&["list", "disable", &id]);
    assert_eq!(code, 5, "protected should exit 5");
    assert!(stderr.starts_with("protected:"), "got: {stderr}");

    // Deleting a protected list must be refused too — this is the path the old
    // CLI bypassed entirely by using SetBlockListEnabled.
    assert_eq!(cli.fail(&["list", "rm", &id]).0, 5);

    let status = cli.json(&["protect", "status"]);
    assert_eq!(status["data"].as_array().unwrap().len(), 1);
    assert_eq!(status["data"][0]["block_list_id"], id);
    assert_eq!(
        status["data"][0]["prevent_modification"], true,
        "protection must default to actually protecting"
    );
}

#[test]
fn protection_opt_outs_are_honoured() {
    let cli = Cli::new();
    let id = cli.make_list("Partly locked");

    cli.ok(&[
        "protect",
        "enable",
        &id,
        "--minutes",
        "60",
        "--allow-modification",
    ]);

    let info = &cli.json(&["protect", "status"])["data"][0];
    assert_eq!(info["prevent_modification"], false);
    assert_eq!(info["prevent_uninstall"], true, "others stay on");

    // With modification permitted, editing the list is allowed again.
    cli.ok(&["site", "add", &id, "example.com"]);
}

#[test]
fn schedule_slots_parse_and_round_trip() {
    let cli = Cli::new();
    let id = cli.make_list("Work");

    cli.ok(&[
        "schedule",
        "set",
        &id,
        "--slot",
        "mon:09:00-17:00",
        "--slot",
        "sat:9-12",
    ]);

    let schedule = &cli.json(&["list", "ls"])["data"][0]["schedule"];
    let slots = schedule["time_slots"].as_array().unwrap();
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0]["day"], "Mon");

    cli.ok(&["schedule", "set", &id, "--always"]);
    assert!(cli.json(&["list", "ls"])["data"][0]["schedule"].is_null());
}

#[test]
fn an_unparseable_slot_is_rejected_before_touching_the_database() {
    let cli = Cli::new();
    let id = cli.make_list("Work");

    let (code, stderr) = cli.fail(&["schedule", "set", &id, "--slot", "funday:9-17"]);
    // clap rejects the value, so this never reaches execute().
    assert_eq!(code, 2, "clap usage errors exit 2");
    assert!(stderr.contains("funday"), "error should name the bad value");
}

#[test]
fn settings_distinguish_unset_from_empty() {
    let cli = Cli::new();

    let unset = cli.json(&["setting", "get", "theme"]);
    assert!(unset["data"].is_null(), "unset should be null, not \"\"");

    cli.ok(&["setting", "set", "theme", ""]);
    let empty = cli.json(&["setting", "get", "theme"]);
    assert_eq!(empty["data"], "", "an explicitly empty value is not null");
}

#[test]
fn allowance_lifecycle_and_validation() {
    let cli = Cli::new();

    let (code, _) = cli.fail(&[
        "allowance",
        "create",
        "youtube.com",
        "--limit-secs",
        "30", // below the 60s minimum
    ]);
    assert_eq!(code, 2);

    let created = cli.json(&[
        "allowance",
        "create",
        "youtube.com",
        "--limit-secs",
        "600",
        "--strict",
    ]);
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let listed = cli.json(&["allowance", "ls"]);
    assert_eq!(listed["data"].as_array().unwrap().len(), 1);
    assert_eq!(listed["data"][0]["remaining_secs"], 600);

    cli.ok(&["allowance", "rm", &id]);
    assert!(
        cli.json(&["allowance", "ls"])["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn pomodoro_reports_idle_then_running() {
    let cli = Cli::new();
    let list = cli.make_list("Focus");

    assert!(cli.json(&["pomodoro", "status"])["data"].is_null());

    cli.ok(&["pomodoro", "start", &list]);

    let status = cli.json(&["pomodoro", "status"]);
    assert_eq!(status["data"]["block_list_name"], "Focus");
    assert_eq!(status["data"]["paused"], false);

    assert_eq!(cli.json(&["pomodoro", "pause"])["data"], true);
    assert_eq!(cli.json(&["pomodoro", "stop"])["data"], true);

    // Stopping again is a no-op and must report that rather than claiming success.
    assert_eq!(cli.json(&["pomodoro", "stop"])["data"], false);
}

#[test]
fn stats_reject_a_backwards_date_range() {
    let cli = Cli::new();

    let (code, stderr) = cli.fail(&[
        "stats",
        "show",
        "--from",
        "2026-05-10",
        "--to",
        "2026-05-01",
    ]);
    assert_eq!(code, 2);
    assert!(stderr.contains("after"), "got: {stderr}");
}

#[test]
fn stats_retention_round_trips() {
    let cli = Cli::new();

    assert_eq!(cli.json(&["stats", "retention"])["data"], 30);
    cli.ok(&["stats", "set-retention", "7"]);
    assert_eq!(cli.json(&["stats", "retention"])["data"], 7);

    assert_eq!(cli.fail(&["stats", "set-retention", "0"]).0, 2);
}

#[test]
fn clear_all_skips_protected_lists() {
    let cli = Cli::new();

    let open = cli.make_list("Open");
    let locked = cli.make_list("Locked");
    for id in [&open, &locked] {
        cli.ok(&["site", "add", id, "x.com"]);
    }
    cli.ok(&["protect", "enable", &locked, "--minutes", "60"]);

    // One locked list must not fail the whole bulk operation.
    assert_eq!(cli.json(&["site", "clear"])["data"], 1);
}

#[test]
fn the_database_flag_isolates_runs_from_each_other() {
    let a = Cli::new();
    let b = Cli::new();

    a.make_list("Only in A");

    assert_eq!(a.json(&["list", "ls"])["data"].as_array().unwrap().len(), 1);
    assert!(
        b.json(&["list", "ls"])["data"]
            .as_array()
            .unwrap()
            .is_empty(),
        "each --db must be a separate database, or tests would interfere"
    );
}

/// Guard against a scenario file drifting from the binary's actual surface.
#[test]
fn every_top_level_subcommand_is_reachable() {
    let cli = Cli::new();
    let help = String::from_utf8_lossy(&cli.raw(&["--help"]).stdout).to_string();

    for expected in [
        "list",
        "site",
        "app",
        "exception",
        "schedule",
        "stats",
        "protect",
        "setting",
        "blocks",
        "pomodoro",
        "allowance",
    ] {
        assert!(
            help.contains(expected),
            "`{expected}` missing from --help output"
        );
    }
}

/// The binary must not need a real Focuser install to run.
#[test]
fn runs_against_a_database_that_does_not_exist_yet() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("nested").join("fresh.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_focuser-cli"))
        .arg("--db")
        .arg(&db)
        .args(["list", "ls"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "a fresh database should be created, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(Path::new(&db).exists(), "database file should be created");
}

// ─── Whole configuration ────────────────────────────────────────────

#[test]
fn export_then_import_round_trips_through_a_file() {
    let source = Cli::new();
    let id = source.make_list("Social");
    source.ok(&["site", "add", &id, "reddit.com"]);

    let exported = source.ok(&["config", "export"]);

    let target = Cli::new();
    target.make_list("Replaced");
    let path = target._dir.path().join("config.json");
    std::fs::write(&path, &exported).unwrap();

    target.ok(&["config", "import", path.to_str().unwrap()]);

    let lists = target.json(&["list", "ls"]);
    let data = lists["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "import replaces rather than merges");
    assert_eq!(data[0]["name"], "Social");
    assert_eq!(data[0]["websites"].as_array().unwrap().len(), 1);
}

#[test]
fn wipe_refuses_without_an_explicit_yes() {
    let cli = Cli::new();
    cli.make_list("Keep me");

    let (code, stderr) = cli.fail(&["config", "wipe"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("--yes"),
        "stderr should say what is missing"
    );

    assert_eq!(
        cli.json(&["list", "ls"])["data"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn wipe_with_yes_removes_everything() {
    let cli = Cli::new();
    cli.make_list("Gone");

    cli.ok(&["config", "wipe", "--yes"]);

    assert!(
        cli.json(&["list", "ls"])["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn import_of_a_junk_file_fails_as_validation_and_changes_nothing() {
    let cli = Cli::new();
    cli.make_list("Keep me");

    let path = cli._dir.path().join("junk.json");
    std::fs::write(&path, "this is not a configuration").unwrap();

    let (code, _) = cli.fail(&["config", "import", path.to_str().unwrap()]);
    assert_eq!(code, 2, "caller error, not a backend fault");
    assert_eq!(
        cli.json(&["list", "ls"])["data"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn wipe_and_import_are_refused_while_a_list_is_locked() {
    let cli = Cli::new();
    let id = cli.make_list("Locked");
    cli.ok(&["protect", "enable", &id, "--minutes", "60"]);

    let path = cli._dir.path().join("empty.json");
    std::fs::write(
        &path,
        r#"{"version":1,"app":"Focuser","exported_at":"2026-07-27T00:00:00Z","block_lists":[]}"#,
    )
    .unwrap();

    assert_eq!(cli.fail(&["config", "wipe", "--yes"]).0, 5);
    assert_eq!(cli.fail(&["config", "import", path.to_str().unwrap()]).0, 5);
    assert_eq!(
        cli.json(&["list", "ls"])["data"].as_array().unwrap().len(),
        1
    );
}

// ─── Diagnostics ────────────────────────────────────────────────────

#[test]
fn check_reports_whether_a_domain_is_blocked() {
    let cli = Cli::new();
    let id = cli.make_list("Social");
    cli.ok(&["site", "add", &id, "reddit.com"]);

    assert_eq!(cli.json(&["check", "reddit.com"])["data"], true);
    assert_eq!(cli.json(&["check", "www.reddit.com"])["data"], true);
    assert_eq!(cli.json(&["check", "example.com"])["data"], false);

    cli.ok(&["list", "disable", &id]);
    assert_eq!(cli.json(&["check", "reddit.com"])["data"], false);
}

#[test]
fn browsers_lists_every_known_browser() {
    let cli = Cli::new();
    let browsers = cli.json(&["browsers"]);

    assert_eq!(browsers["kind"], "browser_status");
    assert!(!browsers["data"].as_array().unwrap().is_empty());
}

#[test]
fn version_matches_the_binary() {
    let cli = Cli::new();
    assert_eq!(cli.ok(&["version"]), env!("CARGO_PKG_VERSION"));
}
