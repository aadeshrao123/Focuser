//! Command dispatch — the single place application behaviour lives.
//!
//! The `match` is exhaustive with no `_` arm on purpose: adding a [`Command`]
//! variant without handling it here is a compile error.

use focuser_common::types::{BlockList, EntityId};
use focuser_core::BlockEngine;

use crate::command::{Command, CommandResult};
use crate::context::AppContext;
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
