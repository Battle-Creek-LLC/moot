//! SQLite store. See SPEC §3 for the schema and PRAGMA settings.
//!
//! Connections are opened with WAL + foreign keys on, and migrations are
//! applied automatically on every command via [`Store::open`]. The schema is
//! versioned with `PRAGMA user_version`; v1 = the SPEC §3.2 layout.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::util::time::now_ms;

mod migrations;
mod model;
mod queries;

pub use model::{Meeting, MeetingFilters, MeetingStatus, NewMeeting};

/// SQLite-backed store. Cheap to clone references; the underlying connection
/// lives for the lifetime of the [`Store`].
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open or create the database at `path`. Runs all pending migrations.
    /// Pass `:memory:` for an in-memory DB (used by tests).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        migrations::run(&conn)?;
        Ok(Store { conn })
    }

    /// In-memory store for tests.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        migrations::run(&conn)?;
        Ok(Store { conn })
    }

    fn configure(conn: &Connection) -> Result<()> {
        // journal_mode is a query pragma — it returns the new mode. We don't
        // care about the value, just that the call succeeds. This is a no-op
        // for in-memory connections.
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn schema_version(&self) -> Result<u32> {
        let v: u32 =
            self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        Ok(v)
    }

    /// Insert a new meeting. Tags are inserted in the same transaction.
    pub fn insert_meeting(&mut self, m: &NewMeeting) -> Result<()> {
        queries::insert_meeting(&mut self.conn, m)
    }

    /// Update mutable fields of an existing meeting (status, transcripts,
    /// notes, timing, participants). Bumps `updated_at`.
    pub fn update_meeting(&mut self, m: &Meeting) -> Result<()> {
        queries::update_meeting(&mut self.conn, m)
    }

    /// Set just the status + updated_at. Used by `run` during polling.
    pub fn set_status(&mut self, id: &str, status: MeetingStatus) -> Result<()> {
        let updated = now_ms();
        let n = self.conn.execute(
            "UPDATE meetings SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), updated, id],
        )?;
        if n == 0 {
            return Err(Error::Db(format!("no meeting with id {id}")));
        }
        Ok(())
    }

    /// Look up a meeting by id or slug. Returns `None` if not found.
    pub fn get_meeting(&self, id_or_slug: &str) -> Result<Option<Meeting>> {
        queries::get_meeting(&self.conn, id_or_slug)
    }

    /// Look up a meeting and error with a Cli error if missing — convenient
    /// for command implementations.
    pub fn require_meeting(&self, id_or_slug: &str) -> Result<Meeting> {
        self.get_meeting(id_or_slug)?
            .ok_or_else(|| Error::Cli(format!("no meeting found for `{id_or_slug}`")))
    }

    /// Filter + sort meetings.
    pub fn list_meetings(&self, filters: &MeetingFilters) -> Result<Vec<Meeting>> {
        queries::list_meetings(&self.conn, filters)
    }

    /// Delete a meeting and any rows referencing it (sessions, tags) via
    /// ON DELETE CASCADE.
    pub fn delete_meeting(&mut self, id: &str) -> Result<()> {
        let n = self
            .conn
            .execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Error::Db(format!("no meeting with id {id}")));
        }
        Ok(())
    }

    /// Tag operations are simple, so live directly on the store.
    pub fn add_tag(&mut self, meeting_id: &str, tag: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO tags (meeting_id, tag) VALUES (?1, ?2)",
            params![meeting_id, tag],
        )?;
        Ok(())
    }

    pub fn tags_for(&self, meeting_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag FROM tags WHERE meeting_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map(params![meeting_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Slug uniqueness check used by [`crate::util::slug::disambiguate`].
    pub fn slug_taken(&self, slug: &str) -> Result<bool> {
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM meetings WHERE slug = ?1",
                params![slug],
                |r| r.get(0),
            )
            .optional()?;
        Ok(exists.is_some())
    }

    // ---- sessions -------------------------------------------------------

    pub fn upsert_session(&mut self, meeting_id: &str, state_json: &str) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sessions (meeting_id, state_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(meeting_id) DO UPDATE SET state_json = excluded.state_json, updated_at = excluded.updated_at",
            params![meeting_id, state_json, now],
        )?;
        Ok(())
    }

    pub fn get_session(&self, meeting_id: &str) -> Result<Option<(String, i64)>> {
        let row = self
            .conn
            .query_row(
                "SELECT state_json, updated_at FROM sessions WHERE meeting_id = ?1",
                params![meeting_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    pub fn delete_session(&mut self, meeting_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        Ok(())
    }

    /// Session rows whose meeting is in a terminal state, plus rows whose
    /// last poll is older than `stale_threshold_ms` (used by `clean`).
    pub fn list_orphan_sessions(
        &self,
        stale_threshold_ms: i64,
    ) -> Result<Vec<(String, MeetingStatus, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.meeting_id, m.status, s.updated_at
             FROM sessions s
             JOIN meetings m ON m.id = s.meeting_id
             WHERE m.status IN ('active', 'failed', 'cancelled')
                OR (m.status IN ('recording', 'processing') AND s.updated_at < ?1)
             ORDER BY s.updated_at ASC",
        )?;
        let rows = stmt.query_map(params![stale_threshold_ms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                MeetingStatus::from_str(&r.get::<_, String>(1)?),
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

/// Default-resolved DB path (XDG + env), with optional `--db` override.
pub fn resolve_path(override_path: Option<&Path>) -> Result<PathBuf> {
    crate::paths::db_path(override_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_store_is_at_v1() {
        let store = Store::in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 1);
    }

    #[test]
    fn meeting_round_trip() {
        let mut store = Store::in_memory().unwrap();
        let id = ulid::Ulid::new().to_string();
        let new = NewMeeting {
            id: id.clone(),
            slug: "test-2026-04-29".into(),
            title: "Test".into(),
            platform: Some("meet".into()),
            url: Some("https://meet.google.com/abc".into()),
            recall_bot_id: None,
            language: None,
            started_at: Some(now_ms()),
            ended_at: None,
            duration_secs: None,
            status: MeetingStatus::Active,
            transcript_jsonl: None,
            transcript_md: None,
            notes_md: None,
            notes_prompt: None,
            participants_json: Some("[]".into()),
        };
        store.insert_meeting(&new).unwrap();

        let got = store.get_meeting(&id).unwrap().unwrap();
        assert_eq!(got.title, "Test");
        assert_eq!(got.slug, "test-2026-04-29");

        let by_slug = store.get_meeting("test-2026-04-29").unwrap().unwrap();
        assert_eq!(by_slug.id, id);
    }

    #[test]
    fn slug_taken_detects_existing() {
        let mut store = Store::in_memory().unwrap();
        let new = NewMeeting {
            id: ulid::Ulid::new().to_string(),
            slug: "dup-2026-04-29".into(),
            title: "x".into(),
            platform: None,
            url: None,
            recall_bot_id: None,
            language: None,
            started_at: None,
            ended_at: None,
            duration_secs: None,
            status: MeetingStatus::Active,
            transcript_jsonl: None,
            transcript_md: None,
            notes_md: None,
            notes_prompt: None,
            participants_json: None,
        };
        store.insert_meeting(&new).unwrap();
        assert!(store.slug_taken("dup-2026-04-29").unwrap());
        assert!(!store.slug_taken("not-there").unwrap());
    }

    #[test]
    fn session_upsert_and_orphan_listing() {
        let mut store = Store::in_memory().unwrap();
        let id = "01ABC".to_string();
        let new = NewMeeting {
            id: id.clone(),
            slug: "foo-2026-04-29".into(),
            title: "x".into(),
            platform: None,
            url: None,
            recall_bot_id: None,
            language: None,
            started_at: None,
            ended_at: None,
            duration_secs: None,
            status: MeetingStatus::Recording,
            transcript_jsonl: None,
            transcript_md: None,
            notes_md: None,
            notes_prompt: None,
            participants_json: None,
        };
        store.insert_meeting(&new).unwrap();
        store.upsert_session(&id, "{}").unwrap();
        let got = store.get_session(&id).unwrap().unwrap();
        assert_eq!(got.0, "{}");

        // Stale threshold in the future → counts as orphan.
        let stale = now_ms() + 10_000;
        let orphans = store.list_orphan_sessions(stale).unwrap();
        assert!(orphans.iter().any(|(mid, _, _)| mid == &id));
    }
}
