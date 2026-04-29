//! Verify a fresh DB lands at user_version=1 with the expected tables and
//! indexes. Catches regressions where a future migration silently drops
//! something.

use moot::store::Store;

#[test]
fn fresh_db_has_v1_schema() {
    let store = Store::in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);

    let conn = store.conn();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(tables.contains(&"meetings".to_string()));
    assert!(tables.contains(&"tags".to_string()));
    assert!(tables.contains(&"sessions".to_string()));

    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(indexes.contains(&"meetings_started_at".to_string()));
    assert!(indexes.contains(&"meetings_status".to_string()));
    assert!(indexes.contains(&"tags_tag".to_string()));
}

#[test]
fn migrations_are_idempotent() {
    // Open and re-open the same in-memory DB twice — the second pass should
    // see user_version = 1 and not try to re-run apply_v1.
    let path = std::env::temp_dir().join(format!(
        "moot-migrations-test-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let _ = Store::open(&path).unwrap();
    }
    {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
    }
    let _ = std::fs::remove_file(&path);
}
