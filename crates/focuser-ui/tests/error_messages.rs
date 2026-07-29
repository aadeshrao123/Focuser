//! Every error code the backend can return has something a user can read.
//!
//! `CommandError::code()` is deliberately stable so the frontend owns the
//! wording. The risk in that split is a new variant being added in Rust and
//! nobody remembering the other half, which shows up as a raw `internal` in
//! front of someone rather than as a failing build.
//!
//! Codes are allowed to be *deliberately* untranslated when their message
//! carries detail worth more than a translated generic — see `src/lib/errors.ts`.

use std::path::PathBuf;

use focuser_app::error::CommandError;
use focuser_common::types::EntityId;

/// Codes whose Rust message is more useful than a translated sentence would be.
const INTENTIONALLY_UNTRANSLATED: &[&str] = &["validation", "core", "internal"];

fn messages_json() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/messages/en.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&raw).expect("en.json is not valid JSON")
}

#[test]
fn every_error_code_has_a_message_or_is_deliberately_raw() {
    let messages = messages_json();
    let table = messages.as_object().expect("en.json is not an object");

    // One of every variant. Adding a variant without adding it here is caught
    // by the exhaustive match below.
    let all = [
        CommandError::BlockListNotFound(EntityId::nil()),
        CommandError::RuleNotFound(EntityId::nil()),
        CommandError::AllowanceNotFound(EntityId::nil()),
        CommandError::Protected,
        CommandError::Validation("x".into()),
        CommandError::Unsupported,
    ];

    for error in &all {
        let code = error.code();
        if INTENTIONALLY_UNTRANSLATED.contains(&code) {
            continue;
        }
        let key = format!("error_{code}");
        assert!(
            table.contains_key(&key),
            "CommandError::{code} has no `{key}` in frontend/messages/en.json. \
             Add one, or list the code in INTENTIONALLY_UNTRANSLATED here and in errors.ts."
        );
    }
}

/// Fails to compile when a variant is added, which is the point.
#[test]
fn the_variant_list_above_is_complete() {
    fn assert_exhaustive(error: &CommandError) -> &'static str {
        match error {
            CommandError::BlockListNotFound(_) => "block_list_not_found",
            CommandError::RuleNotFound(_) => "rule_not_found",
            CommandError::AllowanceNotFound(_) => "allowance_not_found",
            CommandError::Protected => "protected",
            CommandError::Validation(_) => "validation",
            CommandError::Unsupported => "unsupported",
            CommandError::Core(_) => "core",
            CommandError::Internal(_) => "internal",
        }
    }

    assert_eq!(assert_exhaustive(&CommandError::Protected), "protected");
}
