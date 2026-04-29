//! LIKE-based search across `title`, `notes_md`, `transcript_md`. See SPEC
//! §5.8. Multi-word queries are AND'd; quoted phrases require an exact
//! substring.
//!
//! For thousands of meetings this is fine. If it gets slow we add an FTS5
//! virtual table in a v2 migration without changing the CLI surface.

use crate::error::{Error, Result};
use crate::store::{Meeting, MeetingFilters, Store};

/// Which DB columns to scan.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Field {
    Title,
    Notes,
    Transcript,
}

impl Field {
    fn column(&self) -> &'static str {
        match self {
            Field::Title => "title",
            Field::Notes => "notes_md",
            Field::Transcript => "transcript_md",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Field::Title => "title",
            Field::Notes => "notes",
            Field::Transcript => "transcript",
        }
    }
}

pub fn parse_fields(s: &str) -> Result<Vec<Field>> {
    let mut out = Vec::new();
    for raw in s.split(',') {
        let token = raw.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        let f = match token.as_str() {
            "title" => Field::Title,
            "notes" => Field::Notes,
            "transcript" => Field::Transcript,
            other => {
                return Err(Error::Cli(format!(
                    "unknown search field `{other}` (try title,notes,transcript)"
                )));
            }
        };
        if !out.contains(&f) {
            out.push(f);
        }
    }
    if out.is_empty() {
        return Err(Error::Cli("--in must list at least one field".into()));
    }
    Ok(out)
}

/// Tokenize a query. Quoted spans become a single token; unquoted whitespace
/// splits on words. Empty tokens are dropped.
pub fn tokenize(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;
    for c in query.chars() {
        match c {
            '"' => {
                if in_quotes {
                    if !buf.is_empty() {
                        tokens.push(buf.clone());
                        buf.clear();
                    }
                    in_quotes = false;
                } else {
                    in_quotes = true;
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !buf.is_empty() {
                    tokens.push(buf.clone());
                    buf.clear();
                }
            }
            _ => buf.push(c),
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    tokens
}

/// One match in one field of one meeting.
#[derive(Debug, Clone)]
pub struct Match {
    pub field: Field,
    pub snippet: String,
    pub offset: usize,
}

/// One result row.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub meeting: Meeting,
    pub matches: Vec<Match>,
    pub score: usize,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub fields: Vec<Field>,
    pub context_chars: usize,
    pub include_snippets: bool,
    pub limit: Option<usize>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            fields: vec![Field::Title, Field::Notes, Field::Transcript],
            context_chars: 80,
            include_snippets: true,
            limit: None,
        }
    }
}

pub fn search(
    store: &Store,
    query: &str,
    filters: &MeetingFilters,
    opts: &SearchOptions,
) -> Result<Vec<SearchHit>> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Err(Error::Cli("search query is empty".into()));
    }

    let candidates = candidate_meetings(store, &tokens, &opts.fields, filters)?;

    let mut hits = Vec::new();
    for meeting in candidates {
        let mut matches = Vec::new();
        let mut score = 0usize;
        for field in &opts.fields {
            let body = match field {
                Field::Title => Some(meeting.title.as_str()),
                Field::Notes => meeting.notes_md.as_deref(),
                Field::Transcript => meeting.transcript_md.as_deref(),
            };
            let Some(body) = body else { continue };
            let body_lower = body.to_ascii_lowercase();
            // Require all tokens be present in this field for the field to
            // contribute snippets — but counting matches per field still
            // honors the AND across the whole query at candidate time.
            for token in &tokens {
                let needle = token.to_ascii_lowercase();
                let mut start = 0;
                while let Some(rel) = body_lower[start..].find(&needle) {
                    let abs = start + rel;
                    score += 1;
                    if opts.include_snippets {
                        matches.push(Match {
                            field: *field,
                            snippet: snippet(body, abs, needle.len(), opts.context_chars),
                            offset: abs,
                        });
                    }
                    start = abs + needle.len();
                    if start >= body_lower.len() {
                        break;
                    }
                }
            }
        }
        if score > 0 {
            hits.push(SearchHit {
                meeting,
                matches,
                score,
            });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score));
    if let Some(limit) = opts.limit {
        hits.truncate(limit);
    }
    Ok(hits)
}

fn candidate_meetings(
    store: &Store,
    tokens: &[String],
    fields: &[Field],
    filters: &MeetingFilters,
) -> Result<Vec<Meeting>> {
    // Build a SQL where-clause that AND's the filters and OR's the search
    // fields per token. The result set is filtered down by Rust per-row
    // scoring afterwards.
    let mut sql = String::from(
        "SELECT id, slug, title, platform, url, recall_bot_id, language,\
         started_at, ended_at, duration_secs, status, transcript_jsonl, transcript_md,\
         notes_md, notes_prompt, participants_json, created_at, updated_at \
         FROM meetings",
    );
    let mut clauses: Vec<String> = Vec::new();
    let mut bindings: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(ms) = filters.since_ms {
        clauses.push(format!("started_at >= ?{}", bindings.len() + 1));
        bindings.push(rusqlite::types::Value::Integer(ms));
    }
    if let Some(tag) = &filters.tag {
        clauses.push(format!(
            "id IN (SELECT meeting_id FROM tags WHERE tag = ?{})",
            bindings.len() + 1
        ));
        bindings.push(rusqlite::types::Value::Text(tag.clone()));
    }
    if let Some(status) = filters.status {
        clauses.push(format!("status = ?{}", bindings.len() + 1));
        bindings.push(rusqlite::types::Value::Text(status.as_str().into()));
    } else if !filters.include_cancelled {
        clauses.push("status NOT IN ('cancelled', 'failed')".into());
    }

    for token in tokens {
        let pattern = format!("%{}%", escape_like(token));
        let placeholder = bindings.len() + 1;
        let parts: Vec<String> = fields
            .iter()
            .map(|f| format!("LOWER({}) LIKE LOWER(?{}) ESCAPE '\\'", f.column(), placeholder))
            .collect();
        clauses.push(format!("({})", parts.join(" OR ")));
        bindings.push(rusqlite::types::Value::Text(pattern));
    }

    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY COALESCE(started_at, created_at) DESC");

    let mut stmt = store.conn().prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> =
        bindings.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(params_ref.as_slice(), |row| {
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
            status: crate::store::MeetingStatus::from_str(&row.get::<_, String>(10)?),
            transcript_jsonl: row.get(11)?,
            transcript_md: row.get(12)?,
            notes_md: row.get(13)?,
            notes_prompt: row.get(14)?,
            participants_json: row.get(15)?,
            created_at: row.get(16)?,
            updated_at: row.get(17)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            other => out.push(other),
        }
    }
    out
}

fn snippet(body: &str, match_offset: usize, match_len: usize, ctx: usize) -> String {
    // Operate on bytes, but snap to char boundaries.
    let bytes = body.as_bytes();
    let start = match_offset.saturating_sub(ctx);
    let end = (match_offset + match_len + ctx).min(bytes.len());

    let start = floor_char_boundary(body, start);
    let end = ceil_char_boundary(body, end);

    let mut s = String::new();
    if start > 0 {
        s.push('…');
    }
    s.push_str(&body[start..end].replace('\n', " "));
    if end < bytes.len() {
        s.push('…');
    }
    s
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MeetingStatus, NewMeeting};

    fn seed(store: &mut Store, slug: &str, title: &str, transcript: &str, notes: &str) -> String {
        let id = ulid::Ulid::new().to_string();
        store
            .insert_meeting(&NewMeeting {
                id: id.clone(),
                slug: slug.into(),
                title: title.into(),
                platform: Some("meet".into()),
                url: None,
                recall_bot_id: None,
                language: None,
                started_at: Some(1700000000000),
                ended_at: None,
                duration_secs: None,
                status: MeetingStatus::Active,
                transcript_jsonl: None,
                transcript_md: Some(transcript.into()),
                notes_md: Some(notes.into()),
                notes_prompt: None,
                participants_json: None,
            })
            .unwrap();
        id
    }

    #[test]
    fn tokenize_handles_phrases() {
        assert_eq!(
            tokenize("hello \"rate limit\" world"),
            vec!["hello", "rate limit", "world"]
        );
    }

    #[test]
    fn search_scores_more_matches_higher() {
        let mut store = Store::in_memory().unwrap();
        seed(
            &mut store,
            "a-2026",
            "Auth talk",
            "we discussed auth at length",
            "auth notes here",
        );
        seed(
            &mut store,
            "b-2026",
            "Standup",
            "standup notes",
            "no relevant content",
        );

        let hits = search(
            &store,
            "auth",
            &MeetingFilters::default(),
            &SearchOptions::default(),
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].meeting.slug, "a-2026");
        assert!(hits[0].score >= 3);
        assert!(!hits[0].matches.is_empty());
    }

    #[test]
    fn and_across_tokens() {
        let mut store = Store::in_memory().unwrap();
        seed(&mut store, "a-2026", "Auth and rate", "auth rate-limit decision", "");
        seed(&mut store, "b-2026", "Auth only", "we talked about auth", "");
        let hits = search(
            &store,
            "auth rate",
            &MeetingFilters::default(),
            &SearchOptions::default(),
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].meeting.slug, "a-2026");
    }
}
