//! Hand-rolled migrations keyed on `PRAGMA user_version`. Each version is
//! idempotent and runs in a single transaction.

use rusqlite::Connection;

use crate::error::Result;

const LATEST: u32 = 1;

pub fn run(conn: &Connection) -> Result<()> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let mut v = current;
    while v < LATEST {
        match v {
            0 => apply_v1(conn)?,
            other => panic!("no migration for version {other}"),
        }
        v += 1;
    }
    Ok(())
}

fn apply_v1(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        r#"
        CREATE TABLE meetings (
            id                TEXT PRIMARY KEY,
            slug              TEXT UNIQUE NOT NULL,
            title             TEXT NOT NULL,
            platform          TEXT,
            url               TEXT,
            recall_bot_id     TEXT UNIQUE,
            language          TEXT,
            started_at        INTEGER,
            ended_at          INTEGER,
            duration_secs     INTEGER,
            status            TEXT NOT NULL,
            transcript_jsonl  TEXT,
            transcript_md     TEXT,
            notes_md          TEXT,
            notes_prompt      TEXT,
            participants_json TEXT,
            created_at        INTEGER NOT NULL,
            updated_at        INTEGER NOT NULL
        );

        CREATE TABLE tags (
            meeting_id      TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            tag             TEXT NOT NULL,
            PRIMARY KEY (meeting_id, tag)
        );

        CREATE TABLE sessions (
            meeting_id      TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
            state_json      TEXT NOT NULL,
            updated_at      INTEGER NOT NULL
        );

        CREATE INDEX meetings_started_at ON meetings(started_at DESC);
        CREATE INDEX meetings_status ON meetings(status);
        CREATE INDEX tags_tag ON tags(tag);

        PRAGMA user_version = 1;
        "#,
    )?;
    tx.commit()?;
    Ok(())
}
