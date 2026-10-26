use focuser_common::types::{BlockList, Protection};
use focuser_core::{BlockEngine, Database};

#[test]
fn a_corrupt_row_cannot_turn_a_partial_policy_snapshot_into_an_unlock() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("policy.db");
    let db = Database::open(&path).unwrap();
    let valid = BlockList::new("Unprotected list");
    db.create_block_list(&valid).unwrap();
    let mut protected = BlockList::new("Original commitment");
    protected.protection = Some(Protection {
        prevent_uninstall: true,
        prevent_service_stop: true,
        prevent_modification: true,
        started_at: chrono::Utc::now(),
        expires_at: Some(chrono::Utc::now() + chrono::Duration::minutes(5)),
    });
    db.create_block_list(&protected).unwrap();
    let mut engine = BlockEngine::new(db).unwrap();
    let original = serde_json::to_string(engine.block_lists()).unwrap();
    let raw = rusqlite::Connection::open(path).unwrap();
    for corrupt in ["'broken-json'", "X'000102'"] {
        raw.execute(
            &format!("UPDATE block_lists SET data={corrupt} WHERE id=?1"),
            [protected.id.to_string()],
        )
        .unwrap();
        assert!(engine.db().list_block_lists().is_err());
        assert!(engine.refresh().is_err());
        assert_eq!(
            serde_json::to_string(engine.block_lists()).unwrap(),
            original
        );
    }
}
