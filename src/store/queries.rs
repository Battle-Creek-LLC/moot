//! Hand-written SQL for the `meetings` CRUD set.
//!
//! Kept separate from `mod.rs` so the public surface stays readable.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::Result;
use crate::util::time::now_ms;

use super::model::{Meeting, MeetingFilters, MeetingStatus, NewMeeting};

const SELECT_FIELDS: &str = "id, slug, title, platform, url, recall_bot_id, language,\
    started_at, ended_at, duration_secs, status, transcript_jsonl, transcript_md,\
    notes_md, notes_prompt, participants_json, created_at, updated_at";

pub fn insert_meeting(conn: &mut Connection, m: &NewMeeting) -> Result<()> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO meetings (
            id, slug, title, platform, url, recall_bot_id, language,
            started_at, ended_at, duration_secs, status,
            transcript_jsonl, transcript_md, notes_md, notes_prompt,
            participants_json, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15,
            ?16, ?17, ?18
        )",
        params![
            m.id,
            m.slug,
            m.title,
            m.platform,
            m.url,
            m.recall_bot_id,
            m.language,
            m.started_at,
            m.ended_at,
            m.duration_secs,
            m.status.as_str(),
            m.transcript_jsonl,
            m.transcript_md,
            m.notes_md,
            m.notes_prompt,
            m.participants_json,
            now,
            now,
        ],
    )?;
    Ok(())
}

pub fn update_meeting(conn: &mut Connection, m: &Meeting) -> Result<()> {
    let now = now_ms();
    conn.execute(
        "UPDATE meetings SET
            slug = ?2, title = ?3, platform = ?4, url = ?5,
            recall_bot_id = ?6, language = ?7,
            started_at = ?8, ended_at = ?9, duration_secs = ?10,
            status = ?11,
            transcript_jsonl = ?12, transcript_md = ?13,
            notes_md = ?14, notes_prompt = ?15,
            participants_json = ?16,
            updated_at = ?17
         WHERE id = ?1",
        params![
            m.id,
            m.slug,
            m.title,
            m.platform,
            m.url,
            m.recall_bot_id,
            m.language,
            m.started_at,
            m.ended_at,
            m.duration_secs,
            m.status.as_str(),
            m.transcript_jsonl,
            m.transcript_md,
            m.notes_md,
            m.notes_prompt,
            m.participants_json,
            now,
        ],
    )?;
    Ok(())
}

pub fn get_meeting(conn: &Connection, id_or_slug: &str) -> Result<Option<Meeting>> {
    let sql = format!(
        "SELECT {SELECT_FIELDS} FROM meetings WHERE id = ?1 OR slug = ?1 LIMIT 1"
    );
    let row = conn
        .query_row(&sql, params![id_or_slug], from_row)
        .optional()?;
    Ok(row)
}

pub fn list_meetings(conn: &Connection, f: &MeetingFilters) -> Result<Vec<Meeting>> {
    let mut sql = format!("SELECT {SELECT_FIELDS} FROM meetings");
    let mut clauses: Vec<String> = Vec::new();
    let mut bindings: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(ms) = f.since_ms {
        clauses.push(format!("started_at >= ?{}", bindings.len() + 1));
        bindings.push(rusqlite::types::Value::Integer(ms));
    }
    if let Some(tag) = &f.tag {
        clauses.push(format!(
            "id IN (SELECT meeting_id FROM tags WHERE tag = ?{})",
            bindings.len() + 1
        ));
        bindings.push(rusqlite::types::Value::Text(tag.clone()));
    }
    if let Some(status) = f.status {
        clauses.push(format!("status = ?{}", bindings.len() + 1));
        bindings.push(rusqlite::types::Value::Text(status.as_str().into()));
    } else if !f.include_cancelled {
        clauses.push("status != 'cancelled'".into());
    }

    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY COALESCE(started_at, created_at) DESC");
    if let Some(limit) = f.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> =
        bindings.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(params_ref.as_slice(), from_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn from_row(row: &Row<'_>) -> rusqlite::Result<Meeting> {
    let status_str: String = row.get(10)?;
    Ok(Meeting {
        id: row.get(0)?,
        slug: row.get(1)?,
        title: row.get(2)?,
        platform: row.get(3)?,
        url: row.get(4)?,
        recall_bot_id: row.get(5)?,
        language: row.get(6)?,
        started_at: row.get(7)?,
        ended_at: row.get(8)?,
        duration_secs: row.get(9)?,
        status: MeetingStatus::from_str(&status_str),
        transcript_jsonl: row.get(11)?,
        transcript_md: row.get(12)?,
        notes_md: row.get(13)?,
        notes_prompt: row.get(14)?,
        participants_json: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}
