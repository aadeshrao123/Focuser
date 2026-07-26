//! The command surface: every action the application can perform.
//!
//! Adding a variant here is the *only* way to add an operation. Both the Tauri
//! shim layer and the CLI are generated from / dispatch on this enum, so an
//! operation cannot exist in one frontend and not the other.
//!
//! Ported incrementally from `focuser-ui/src/commands.rs` — see
//! `internal-docs/tasks/2026-07-26-ui-revamp/02-command-core.md` for the order.

use focuser_common::types::{BlockList, EntityId};
use serde::{Deserialize, Serialize};
use specta::Type;

/// One application action.
///
/// Adjacently tagged so the wire form is `{"cmd": "...", "args": {...}}` — stable,
/// self-describing, and a clean discriminated union in TypeScript.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "cmd", content = "args", rename_all = "snake_case")]
pub enum Command {
    // ─── Block lists ──────────────────────────────────────────────
    /// All block lists, with their rules.
    ListBlockLists,
    /// Create an empty block list. Returns the created list.
    CreateBlockList { name: String },
    /// Replace a block list wholesale.
    ///
    /// Takes a real [`BlockList`], not the JSON string the old
    /// `update_block_list(list_json: String)` accepted.
    UpdateBlockList { list: Box<BlockList> },
    /// Delete a block list and re-sync the hosts file.
    DeleteBlockList { id: EntityId },
    /// Enable or disable a block list.
    ToggleBlockList { id: EntityId, enabled: bool },
}

/// The result of a successful [`Command`].
///
/// Deliberately typed — the old command layer returned `serde_json::Value` from
/// 24 of its 55 commands, which meant the frontend hand-decoded shapes that the
/// compiler never checked.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum CommandResult {
    /// Succeeded, nothing to return.
    Unit,
    BlockList(Box<BlockList>),
    BlockLists(Vec<BlockList>),
}

impl CommandResult {
    /// Convenience for tests and callers that know the shape they expect.
    pub fn as_block_lists(&self) -> Option<&[BlockList]> {
        match self {
            Self::BlockLists(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_block_list(&self) -> Option<&BlockList> {
        match self {
            Self::BlockList(l) => Some(l),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_round_trips_through_json() {
        let cmd = Command::ToggleBlockList {
            id: EntityId::nil(),
            enabled: true,
        };
        let json = serde_json::to_string(&cmd).unwrap();

        assert!(
            json.contains(r#""cmd":"toggle_block_list""#),
            "expected adjacent tagging, got {json}"
        );

        let back: Command = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            Command::ToggleBlockList { enabled: true, .. }
        ));
    }

    #[test]
    fn unit_variant_serialises_without_args() {
        let json = serde_json::to_string(&Command::ListBlockLists).unwrap();
        assert_eq!(json, r#"{"cmd":"list_block_lists"}"#);
    }
}
