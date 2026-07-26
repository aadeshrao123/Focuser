//! Command errors.
//!
//! Replaces the `Result<T, String>` used throughout the old Tauri command layer.
//! Each variant carries a stable machine-readable `code()`, which gives the CLI
//! distinct exit codes and lets the frontend branch on failure kind without
//! string-matching human-readable messages.

use focuser_common::EntityId;
use serde::Serialize;

/// Anything a command can fail with.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("block list not found: {0}")]
    BlockListNotFound(EntityId),

    #[error("rule not found: {0}")]
    RuleNotFound(EntityId),

    #[error("allowance not found: {0}")]
    AllowanceNotFound(EntityId),

    #[error("block list is protected and cannot be modified")]
    Protected,

    #[error("invalid input: {0}")]
    Validation(String),

    #[error("operation is not supported on this platform")]
    Unsupported,

    #[error(transparent)]
    Core(focuser_common::FocuserError),

    #[error("{0}")]
    Internal(String),
}

/// Classify core errors instead of funnelling them all into [`CommandError::Core`].
///
/// Without this, a caller mistake like "delete a list that doesn't exist" arrives
/// as `Core`, which reports exit code 1 (internal fault) and HTTP 500 — telling a
/// script the backend broke when in fact the request was wrong. The distinction is
/// the whole point of having stable codes, so the mapping has to happen here rather
/// than at each call site.
impl From<focuser_common::FocuserError> for CommandError {
    fn from(err: focuser_common::FocuserError) -> Self {
        use focuser_common::FocuserError as E;
        match err {
            E::BlockListNotFound(id) | E::BlockNotFound(id) => {
                // The core carries ids as strings; keep the original text when it
                // isn't a well-formed UUID rather than inventing a nil id.
                match id.parse::<EntityId>() {
                    Ok(parsed) => Self::BlockListNotFound(parsed),
                    Err(_) => Self::Validation(format!("block list not found: {id}")),
                }
            }
            E::LockActive(_) | E::ProtectionActive(_) => Self::Protected,
            E::InvalidPattern(m) | E::InvalidConfig(m) => Self::Validation(m),
            other => Self::Core(other),
        }
    }
}

impl CommandError {
    /// Stable identifier for this failure kind. Safe to branch on; never localised.
    pub fn code(&self) -> &'static str {
        match self {
            Self::BlockListNotFound(_) => "block_list_not_found",
            Self::RuleNotFound(_) => "rule_not_found",
            Self::AllowanceNotFound(_) => "allowance_not_found",
            Self::Protected => "protected",
            Self::Validation(_) => "validation",
            Self::Unsupported => "unsupported",
            Self::Core(_) => "core",
            Self::Internal(_) => "internal",
        }
    }

    /// Process exit code for the CLI. Distinct per kind so scripts can branch.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::BlockListNotFound(_) | Self::RuleNotFound(_) | Self::AllowanceNotFound(_) => 4,
            Self::Protected => 5,
            Self::Validation(_) => 2,
            Self::Unsupported => 6,
            Self::Core(_) | Self::Internal(_) => 1,
        }
    }
}

/// Wire representation: `{ "code": "...", "message": "..." }`.
///
/// Hand-written rather than derived because `FocuserError` is not `Serialize`,
/// and because the shape should stay stable even as variants are added.
impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("CommandError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type CommandOutcome<T> = Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_distinct_per_kind() {
        let errors = [
            CommandError::BlockListNotFound(EntityId::nil()),
            CommandError::Protected,
            CommandError::Validation("bad".into()),
            CommandError::Unsupported,
            CommandError::Internal("boom".into()),
        ];
        let codes: Vec<_> = errors.iter().map(|e| e.code()).collect();
        let mut unique = codes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            codes.len(),
            unique.len(),
            "codes must be unique per variant"
        );
    }

    #[test]
    fn validation_exits_two_and_protected_exits_five() {
        assert_eq!(CommandError::Validation("x".into()).exit_code(), 2);
        assert_eq!(CommandError::Protected.exit_code(), 5);
    }

    #[test]
    fn core_not_found_is_classified_as_not_found_not_internal() {
        let id = EntityId::new_v4();
        let err: CommandError =
            focuser_common::FocuserError::BlockListNotFound(id.to_string()).into();

        assert_eq!(err.code(), "block_list_not_found");
        assert_eq!(
            err.exit_code(),
            4,
            "a missing list is a caller error, not an internal fault"
        );
    }

    #[test]
    fn core_protection_errors_map_to_protected() {
        let err: CommandError =
            focuser_common::FocuserError::ProtectionActive("still locked".into()).into();
        assert_eq!(err.code(), "protected");
        assert_eq!(err.exit_code(), 5);
    }

    #[test]
    fn genuinely_internal_core_errors_stay_internal() {
        let err: CommandError = focuser_common::FocuserError::Database("disk gone".into()).into();
        assert_eq!(err.code(), "core");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn serialises_to_code_and_message() {
        let json = serde_json::to_value(CommandError::Protected).unwrap();
        assert_eq!(json["code"], "protected");
        assert!(
            json["message"].as_str().unwrap().contains("protected"),
            "message should be human readable, got {json:?}"
        );
    }
}
