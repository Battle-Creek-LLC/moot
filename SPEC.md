# Moot — Design Spec

**Version:** 0.1.0 (draft)
**Status:** Pre-implementation
**License:** MIT

Moot is a standalone CLI that dispatches a [Recall.ai](https://recall.ai) bot to a video call, captures the transcript, and optionally generates notes via the Claude CLI. Captured meetings are stored in a single SQLite database. Moot has no dependencies on the Terra platform — downstream integrations (e.g. creating Terra artifacts from a meeting) are the job of an agent or script that reads from Moot's database or its `export` output.

---

## 1. Goals & Non-Goals

### Goals
- One binary, one SQLite file. No daemons, no servers.
- Useful in isolation: produces a transcript and notes file with no external system required.
- Cross-platform: macOS and Debian/Ubuntu.
- Resumable: a crashed `run` can be picked back up where it left off.
- Agent-friendly: stable JSON output on every command, plus an `export` verb that writes plain files.

### Non-Goals
- Not a transcription engine. Moot delegates capture to Recall.ai.
- Not a graph database. No concept extraction, no edges, no enrichment.
- Not a Terra client. It does not emit events, sign payloads, or call Sunlight.
- Not a real-time tool. v0.1 is poll-based; webhook/streaming is deferred.
- Not multi-user. Single user, single machine, single DB.

---

## 2. Architecture

```
┌─────────────────────────────────────────────┐
│              moot (binary)              │
├─────────────────────────────────────────────┤
│  cli       clap subcommand tree             │
│  recall    Recall.ai HTTP client            │
│  notes     `claude --print` wrapper         │
│  store     SQLite (rusqlite, bundled)       │
│  search    LIKE-based search + snippets     │
│  session   Resume state (in DB)             │
│  bundle    Export to files (TOML + MD)      │
│  paths     XDG-aware path resolution        │
│  secrets   OS keychain access               │
└─────────────────────────────────────────────┘
        │                    │
        ▼                    ▼
   Recall.ai API        ~/.../moot.db
   (HTTPS + bearer)     (SQLite, WAL mode)
```

Single-binary, single-crate Rust application. Async runtime is `tokio` (needed by `reqwest`). SQLite is statically linked via `rusqlite` with the `bundled` feature so installation never depends on a system libsqlite.

---

## 3. Storage

### 3.1 Database location (XDG)

Resolved via the `directories` crate (`ProjectDirs::from("dev", "battlecreek", "moot")` — produces the bundle id `dev.battlecreek.moot` on macOS shown below):

| OS      | Default DB path                                                 |
|---------|-----------------------------------------------------------------|
| Linux   | `$XDG_DATA_HOME/moot/moot.db` (`~/.local/share/moot/moot.db`) |
| macOS   | `~/Library/Application Support/dev.battlecreek.moot/moot.db`              |

Override with `--db <path>` on any command, or `MOOT_DB` env var.

Config file (optional, for default flags) at `ProjectDirs::config_dir()/config.toml`.

### 3.2 Schema (v1)

Deliberately minimal: one row per meeting, transcripts stored as text, no FTS5, no triggers.

```sql
PRAGMA user_version = 1;
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE meetings (
    id                TEXT PRIMARY KEY,        -- ULID
    slug              TEXT UNIQUE NOT NULL,    -- e.g. "weekly-staff-2026-04-29"
    title             TEXT NOT NULL,
    platform          TEXT,                    -- meet | teams | zoom | unknown
    url               TEXT,
    recall_bot_id     TEXT UNIQUE,             -- null for `import`
    language          TEXT,
    started_at        INTEGER,                 -- unix epoch ms
    ended_at          INTEGER,
    duration_secs     INTEGER,
    status            TEXT NOT NULL,           -- recording|processing|active|failed|cancelled
    transcript_jsonl  TEXT,                    -- raw, one utterance per line
    transcript_md     TEXT,                    -- pre-rendered "Speaker (mm:ss): text"
    notes_md          TEXT,
    notes_prompt      TEXT,
    participants_json TEXT,                    -- JSON array of {name, email?}
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
```

Participants are stored as a JSON blob rather than a normalized table — cheaper, and we never query by participant in v0.1 except as a search filter, which scans anyway. If participant-based queries become hot, normalize in a later migration.

A future v2 migration can add an FTS5 virtual table over `title`, `notes_md`, and `transcript_md` if `LIKE`-based search becomes too slow. Non-breaking.

### 3.3 Migrations

Hand-rolled, keyed off `PRAGMA user_version`. Each version is an idempotent `apply_vN()` function. v0 → v1 is the schema above. Migrations run automatically on every command.

### 3.4 Concurrency

WAL mode is enabled. Multiple read commands (`list`, `show`, `export`) run concurrently with a `run` in progress. Two simultaneous `run` invocations are allowed (different meetings) but `run --resume <id>` takes an advisory lock on the row.

---

## 4. Configuration & Secrets

### 4.1 Secrets (OS keychain)

The Recall.ai API key is stored in the OS keychain via the `keyring` crate (v3):

| OS      | Backend                                        |
|---------|------------------------------------------------|
| macOS   | Keychain Services (`security` framework)       |
| Linux   | Secret Service / libsecret (D-Bus)             |

Service name: `moot`. Entry name: `recall-api-key`.

Env fallback: `MOOT_RECALL_API_KEY` is consulted if the keychain entry is absent. Useful for CI and headless installs.

Moot never writes secrets to disk and never stores them in the DB.

### 4.2 Config file

Optional TOML at `<config_dir>/config.toml`:

```toml
default_language = "en"
default_bot_name = "Moot"
notes_default = false
notes_prompt_path = "~/.config/moot/prompts/notes.md"
recall_region = "us-west-2"
```

CLI flags override config; config overrides built-in defaults.

### 4.3 Environment variables

| Var | Effect |
|---|---|
| `MOOT_DB` | Override DB path |
| `MOOT_RECALL_API_KEY` | API key fallback |
| `MOOT_RECALL_REGION` | Recall.ai region |
| `MOOT_LOG` | `tracing` filter (default `info`) |
| `MOOT_NO_COLOR` | Disable terminal colors |

---

## 5. CLI Surface

```
moot <COMMAND>

run         Dispatch a bot to a live meeting
fetch       Re-pull a transcript from a known Recall.ai bot ID
import      Load a meeting from an existing transcript file
notes       Generate or regenerate notes for a captured meeting
export      Write a captured meeting to files on disk
list        List captured meetings
show        Show a single meeting
search      Find meetings by content
clean       Remove old sessions or bundles
secret      Manage credentials in the OS keychain
```

Global flags (apply to every command): `--db <path>`, `--json`, `-v/--verbose`, `-h/--help`, `-V/--version`.

### 5.1 `run`

```
moot run --url <url> [--title <t>] [--platform <p>]
            [--bot-name <n>]   [--language <lang>]
            [--notes]          [--notes-prompt <file>]
            [--resume <id>]    [--dry-run]
```

Flow:
1. Resolve API key (keychain → env). Error if absent.
2. If `--resume <id>`: load `sessions.state_json` row, jump to step 5.
3. Detect platform from URL (override with `--platform`).
4. POST to Recall.ai `/api/v1/bot/`, get `bot_id`. Insert `meetings` row with `status='recording'` and persist `sessions.state_json`.
5. Poll `/api/v1/bot/<id>` every 15s. Update `meetings.status` as it transitions (`recording` → `processing` once Recall reports `call_ended`).
6. On `done`, fetch transcript, bulk-insert `utterances`, populate `participants`, set `started_at`/`ended_at`/`duration_secs`.
7. If `--notes`, generate notes via Claude (§7). Update `notes_md`.
8. Set `status='active'`, delete `sessions` row, rebuild FTS row.
9. Print meeting ID and slug (or full record with `--json`).

`--dry-run` validates config and Recall.ai auth without dispatching a bot.

**Cancellation.** SIGINT (Ctrl-C) during `run` triggers a graceful cancel: the bot is deleted via `DELETE /api/v1/bot/<id>/`, the meeting row transitions to `status='cancelled'`, the `sessions` row is removed, and the process exits with code 130. A second SIGINT exits immediately without cleanup (the row will be left in `recording` until `clean`).

### 5.2 `fetch`

```
moot fetch --bot-id <id> [--title <t>] [--notes]
```

For bots already dispatched outside of `moot run`, or to re-pull a transcript within Recall.ai's 72-hour retention window. Skips dispatch and polling; goes straight to transcript fetch + insert.

### 5.3 `import`

```
moot import -f <path> --title <t> [--platform <p>]
                [--participants <csv>] [--started-at <iso8601>]
                [--notes] [--notes-file <path>]
```

Reads a transcript file with auto-detected format:

| Extension | Format |
|---|---|
| `.jsonl` | One utterance per line (Recall.ai schema) |
| `.vtt`   | WebVTT |
| `.srt`   | SubRip |
| `.txt`   | Plain text, one line per utterance, optional `Speaker:` prefix |

`--notes-file` skips note generation and stores the provided markdown directly.

### 5.4 `notes`

```
moot notes <id-or-slug> [--prompt <file>] [--force]
```

Generate or regenerate notes for a captured meeting. Reads `transcript_md` from the DB; never calls Recall.ai. Writes the result to `meetings.notes_md` and records the prompt used in `notes_prompt`.

- `--prompt <file>`: override the default notes prompt template for this run.
- `--force`: overwrite existing `notes_md` without prompting. Without `--force`, refuses to overwrite a non-null value.

Use this to:
- Add notes to a meeting captured with `run` / `fetch` / `import` (where `--notes` was not passed or failed).
- Try a different prompt template against an existing meeting.
- Recover after a transient Claude CLI failure during `run`.

Same `claude --print` shell-out and same failure semantics as §7. Exit code 4 if the Claude CLI is missing or fails.

### 5.5 `export`

```
moot export <id-or-slug> [--out <dir>|-] [--format jsonl|md|all] [--force]
```

Writes the meeting to a directory:

```
<out>/<slug>/
  meeting.toml      Title, platform, participants, timings, tags
  transcript.jsonl  One utterance per line
  transcript.md     Human-readable rendering with speaker labels
  notes.md          (omitted if notes_md is null)
```

- **Default `--out` is the current directory.** `moot export <id>` from `~/work` writes `~/work/<slug>/`. Predictable, local, no magic paths.
- **`--out -`** writes a tar stream to stdout (for agents and pipelines). Mutually exclusive with `--force`. Stderr still carries logs.
- **`--force`** overwrites an existing `<slug>/` directory. Without it, `export` exits with code 1 if the target already exists, to prevent silent overwrites.
- **`--format md`** writes only `transcript.md` + `notes.md`. **`--format jsonl`** writes only the JSONL. Default `all` writes everything.

This is the primary surface for downstream agents. An agent creating Terra artifacts can either:
- `moot export <id> --out /tmp/m`, then read the four files and run `sp` commands; or
- `moot export <id> --out -` to stream the bundle as a tarball into another tool.

### 5.6 `list`

```
moot list [--since <dur>] [--tag <t>] [--status <s>]
              [--limit <n>] [--all] [--json]
```

Browse and filter captured meetings. `--since` accepts `7d`, `2w`, `1mo`, ISO durations. `cancelled` meetings are hidden by default; `--status cancelled` or `--all` surfaces them. For free-text content search across title, notes, and transcripts, see `moot search`.

Default columns: `id`, `slug`, `title`, `started_at`, `duration`, `status`. `--json` emits an array of full records.

### 5.7 `show`

```
moot show <id-or-slug> [--transcript] [--notes] [--json]
```

Default output: metadata table only. `--transcript` streams `utterances` to stdout. `--notes` prints `notes_md`.

### 5.8 `search`

```
moot search <query> [--in <fields>] [--since <dur>] [--tag <t>]
                        [--participant <name>] [--status <s>]
                        [--limit <n>] [--context <n>] [--no-snippets]
                        [--json]
```

Find captured meetings by content. Matches against `title`, `notes_md`, and `transcript_md`. Each result is a meeting plus one or more snippets showing the matched context.

- `<query>`: free-text query. Multi-word queries are AND'd. Quote a phrase (`"rate limit"`) to require an exact substring.
- `--in <fields>`: comma-separated subset of `title,notes,transcript`. Default: all three.
- `--context <n>`: snippet length in chars around each match. Default 80.
- `--no-snippets`: print meeting rows only, no surrounding text.
- Filter flags (`--since`, `--tag`, `--participant`, `--status`) compose with the query as additional `WHERE` clauses.
- Default `--status active`. `cancelled` and `failed` meetings are excluded unless explicitly requested.

Implementation: case-insensitive `LIKE '%term%'` across the searched fields. Score = total match count, sort descending. Snippets = locate each match offset and slice ±`--context` chars. Suitable for thousands of meetings; if it gets slow, an FTS5 virtual table can be added in a v2 migration without changing the CLI.

Default human output:
```
weekly-staff-2026-04-29 · Weekly Staff Sync · 47m
  …I think we should defer the auth decision until Q3, since the…
  …Bob: but the auth decision blocks the rate-limit work too…

infra-sync-2026-04-22 · Infra Sync · 32m
  …revisit the auth decision once we know more about SCIM…
```

JSON output is an array of `{meeting, matches: [{field, snippet, offset}], score}`.

### 5.9 `clean`

```
moot clean [--sessions] [--older-than <dur>] [--dry-run]
```

`--sessions` (default if no other flag): delete `sessions` rows for meetings with status `active`, `failed`, or `cancelled`, plus any orphaned `recording`/`processing` rows whose session has not been polled in over 24 hours. `--older-than 90d`: cascade-delete meetings older than the duration.

By default, `cancelled` meetings are hidden from `moot list` output. Pass `--status cancelled` (or `--all`) to surface them.

### 5.10 `secret`

```
moot secret set                 # read key from stdin
moot secret get [--reveal]      # masked status, full value with --reveal
moot secret delete
moot secret check               # verify against Recall.ai API
```

---

## 6. Recall.ai Integration

Wraps the [Recall.ai REST API](https://docs.recall.ai). Async client built on `reqwest`. **Auth header is `Authorization: Token <api_key>`** (Recall uses `Token`, not `Bearer`).

### 6.1 Base URL

`https://{region}.recall.ai/api/v1` — region is the *subdomain*, not a path segment. Default region `us-west-2`. Override with `MOOT_RECALL_REGION` or config `recall_region`.

### 6.2 Endpoints

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/bot` | Dispatch a bot |
| `GET`  | `/bot/{id}` | Poll status, find transcript URL |
| `GET`  | `<download_url>` | Download transcript (URL embedded in bot detail; **no auth header needed**) |
| `DELETE` | `/bot/{id}` | Cancel / cleanup |

There is no separate transcript endpoint. The transcript URL is inside the bot detail response at `recordings[0].media_shortcuts.transcript.data.download_url` and is only populated once status reaches `done`.

### 6.3 Bot creation request body

```json
{
  "meeting_url": "https://meet.google.com/...",
  "bot_name": "Moot",
  "recording_config": {
    "transcript": {
      "provider": { "recallai_streaming": {} }
    },
    "retention": { "type": "timed", "hours": 72 }
  }
}
```

The `recording_config` block is mandatory — without it the bot dispatches but produces no transcript. Retention of 72h matches Recall.ai's media retention.

### 6.4 Bot status

The bot detail response includes `status_changes: [{code, sub_code}]`. The latest entry's `code` is the live status. Known codes:

| Code | Meaning | Moot status |
|---|---|---|
| `joining_call` | Bot connecting | `recording` |
| `in_waiting_room` | Awaiting host admit | `recording` |
| `in_call_not_recording` | Joined, pre-record | `recording` |
| `in_call_recording` | Actively recording | `recording` |
| `call_ended` | Meeting over, transcript not ready | `processing` |
| `done` | Transcript ready | `processing` → `active` after fetch |
| `fatal` | Bot failed | `failed` |

### 6.5 Transcript download format

The download URL returns a JSON array. Each entry is one participant's continuous turn:

```json
[
  {
    "participant": { "id": 0, "name": "Alice" },
    "words": [
      { "text": "Morning", "start_timestamp": {"relative": 1.20}, "end_timestamp": {"relative": 1.55} },
      { "text": "everyone", "start_timestamp": {"relative": 1.55}, "end_timestamp": {"relative": 1.92} }
    ]
  }
]
```

Moot assembles each entry into a single utterance by joining `words[].text` with spaces, taking `start = words[0].start_timestamp.relative` and `end = words[-1].end_timestamp.relative`. The result is rendered to `transcript_md` as `**Alice** (00:01): Morning everyone.`.

### 6.6 Polling cadence

15 seconds during the entire `recording` umbrella (`joining_call` through `in_call_recording`). 5 seconds during `processing` (`call_ended` waiting for `done`). Backoff to 60s after 30 minutes with no status change. Hard cap: 12 hours per session.

### 6.7 Failure handling

Transient HTTP errors (5xx, network, 429) retry with exponential backoff (max 5 attempts, base 2s, cap 60s). Permanent errors (4xx other than 429) mark the meeting `failed` and surface the upstream error body in the log. A `fatal` status from Recall.ai is treated as permanent.

---

## 7. Notes Generation

Moot does not integrate with the Anthropic SDK directly. It shells out to the Claude CLI:

```
claude --print < <rendered-prompt>
```

The default prompt template ships embedded in the binary at `prompts/notes.md.tera` (ported from the legacy `sprout meeting` implementation). It uses [Tera](https://keats.github.io/tera/) templating with these variables:

| Variable | Value |
|---|---|
| `{{ title }}` | Meeting title |
| `{{ platform }}` | `meet` / `teams` / `zoom` / `unknown` |
| `{{ meeting_id }}` | Moot ULID |
| `{{ speakers }}` | Comma-separated speaker names |
| `{{ transcript }}` | Full `transcript_md` content |
| `{{ transcript_path }}` | Path to a temp file containing the transcript (for very long meetings — Claude reads the file rather than the prompt body) |
| `{{ output_path }}` | Path to write notes to (when used with claude file output) |

Override with `--notes-prompt <file>` (per invocation) or `notes_prompt_path` in the config file.

Generation is best-effort: a Claude CLI failure during `run`, `fetch`, or `import` logs a warning, leaves `notes_md` null, and does not fail the overall capture. The transcript is the irreplaceable artifact (Recall.ai retains it for 72h); notes can always be regenerated later from the stored transcript. To add or regenerate notes for an existing meeting, use `moot notes <id>` (§5.4) — it reads `transcript_md` from the DB and never calls Recall.ai.

---

## 8. Resume Semantics

Every `run` writes a row in `sessions` after creating the bot:

```json
{
  "meeting_id": "01HXYZ...",
  "phase": "polling",
  "bot_id": "abc-123",
  "platform": "meet",
  "url": "https://meet.google.com/...",
  "started_at_ms": 1735000000000,
  "last_status": "in_call_recording",
  "last_polled_ms": 1735000900000
}
```

The row is updated on every poll. `moot run --resume <meeting-id>` reads the row, re-establishes the Recall.ai client, and continues from `phase`. On successful completion (status `active`) the row is deleted.

`moot list --status recording` surfaces orphaned sessions (process killed mid-poll) for manual `--resume` or `clean`.

---

## 9. Output Formats

### 9.1 `meeting.toml` (export)

```toml
id = "01HXYZ..."
slug = "weekly-staff-2026-04-29"
title = "Weekly Staff Sync"
platform = "meet"
url = "https://meet.google.com/abc-defg-hij"
recall_bot_id = "abc-123"
language = "en"
started_at = "2026-04-29T15:00:00Z"
ended_at = "2026-04-29T15:47:23Z"
duration_secs = 2843
status = "active"
participants = ["Alice", "Bob", "Carol"]
tags = ["staff", "weekly"]
```

### 9.2 `transcript.jsonl` (export & import)

```jsonl
{"idx":0,"speaker":"Alice","ts_offset_ms":1200,"text":"Morning everyone."}
{"idx":1,"speaker":"Bob","ts_offset_ms":4100,"text":"Hey."}
```

### 9.3 `transcript.md` (export)

```markdown
# Weekly Staff Sync
*2026-04-29 · 47m · Google Meet*

**Alice** (00:01): Morning everyone.
**Bob** (00:04): Hey.
```

### 9.4 `--json` output (every command)

Full meeting records use the same shape as the DB row, with `participants`, `tags`, and `utterance_count` joined in. Times are ISO-8601 strings, not epoch ms, in JSON output.

---

## 10. Error Model

Single error enum (`thiserror`) with these variants: `Config`, `Keychain`, `Recall`, `Db`, `Notes`, `Fs`, `Cli`. Every variant carries the underlying error and a stable message. Exit codes:

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic runtime error |
| 2 | CLI usage error (clap) |
| 3 | Configuration error (missing API key, unwritable DB path) |
| 4 | Recall.ai error (bot dispatch failed, transcript unavailable) |
| 5 | Database error |

JSON mode emits `{"error": {"code": "...", "message": "..."}}` and uses the same exit code.

---

## 11. Logging

`tracing` + `tracing-subscriber`. Default level `info` to stderr (human-formatted). `MOOT_LOG=debug` for verbose. `--json` mode flips logs to structured JSON on stderr; stdout stays reserved for command output.

---

## 12. Testing

- **Unit:** schema migrations, FTS triggers, prompt rendering, format parsers (VTT/SRT/JSONL/TXT), CLI argument parsing.
- **Integration:** `import` end-to-end with fixture transcripts. `export` round-trip (`import` then `export`, verify byte-for-byte). `list --search` against a seeded DB.
- **Recall client:** trait-mocked. Real Recall.ai integration tests gated behind `MOOT_RECALL_INTEGRATION=1` (requires a real key).

---

## 13. Release & Versioning

- Semver. v0.x is pre-stable; schema migrations may be breaking until v1.0.
- **CHANGELOG.md** maintained by hand in [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format. One entry per release with sections (Added / Changed / Fixed / Security).
- **Conventional commits** recommended (`feat:`, `fix:`, `chore:`, `docs:`) but not enforced via tooling. Documented in `CONTRIBUTING.md`.
- **CI workflow** (`.github/workflows/ci.yml`) runs on every PR and push to `main`: `cargo build --locked` and `cargo test --locked` on Ubuntu. Mirrors `repocat`'s minimal shape.
- **Release workflow** (`.github/workflows/release.yml`) triggers on tag `v*`. Matrix builds and uploads binaries via [`taiki-e/upload-rust-binary-action`](https://github.com/taiki-e/upload-rust-binary-action) for five targets:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- **`Cross.toml`** installs `libdbus-1-dev:$CROSS_DEB_ARCH` for the aarch64-linux build (required by the keychain crate).
- **crates.io publish** as a final step in `release.yml`, gated on `secrets.CARGO_REGISTRY_TOKEN`. Enables `cargo install moot` for users without Homebrew.
- **Homebrew tap** at `sprouted-dev/homebrew-tap` gets a `moot.rb` formula pointing at the release tarballs, enabling `brew install sprouted-dev/tap/moot`.
- **MSRV**: not pinned explicitly. Crate uses `edition = "2024"`, which requires Rust 1.85+. README documents "current stable Rust" as the floor.
- License: MIT (root `LICENSE`).

---

## 14. Reusing Legacy Code

The Recall.ai integration and notes prompt are ported from `sprouted-dev/terra-legacy`, branch `feat/meeting-module`, path `sprout/src/commands/meeting/`. Files live under `.legacy-reference/meeting/` in this repo as ground-truth references during porting; they are excluded from the build.

| Legacy file | Disposition |
|---|---|
| `recall.rs` | Direct port → `src/recall/mod.rs`. Real Recall.ai client with confirmed API shapes. |
| `models.rs` | Partial port. Lift `Segment`, `BotStatus`, `SessionPhase`. Drop the multi-artifact `ArtifactType` enum (v0.1 is notes-only). |
| `claude.rs` | Adapt → `src/notes/claude.rs`. `claude --print` shell-out wrapper. |
| `prompts/notes.md.tera` | Ship as `prompts/notes.md.tera` (embedded via `include_str!`). |
| `prompts/{spec,adr,journey_map,action_items}.md.tera` | Held for future v0.2+ multi-artifact support. Not in v0.1 binary. |
| `buffer.rs` | Skipped. Real-time JSONL append buffer for streaming — explicitly deferred (§1 non-goals). |
| `session.rs`, `clean.rs`, `export.rs`, `run.rs`, `publisher.rs`, `config.rs`, `artifacts.rs`, `mod.rs` | Reference only. Moot's storage model (SQLite) and CLI surface differ; flow logic informs ours but does not port directly. |

The legacy code targets a 2024-edition Rust crate using `reqwest` (async), `serde`, `anyhow`, `tracing`, and `tera`. All match Moot's planned dependency set.

## 15. Roadmap (post-v0.1)

- Webhook support for real-time transcript delivery (replaces polling).
- Multiple notes prompts per meeting (`--notes-set summary,decisions`).
- Recall.ai bot region auto-detect.
- Optional encryption of `notes_md` and `utterances.text` at rest.
- `moot serve` HTTP read-only API for agent consumption (alternative to `export`).
