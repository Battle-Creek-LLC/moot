# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- CLI command tree: ten subcommands (`run`, `fetch`, `import`, `notes`,
  `export`, `list`, `show`, `search`, `clean`, `secret`) with full flag sets
  per SPEC §5. All execute paths print "not yet implemented" and exit 1
  pending Phase 3.
- `error` module with the seven-variant `thiserror` enum and stable exit
  codes per SPEC §10.
- `logging` module wiring `tracing-subscriber` to `MOOT_LOG`,
  `MOOT_NO_COLOR`, and the global `--json` flag.
- `paths` module resolving the SQLite DB path via `directories::ProjectDirs`
  with `--db` and `MOOT_DB` overrides.
- `secrets` module wrapping the OS keychain (`keyring` crate) with
  `MOOT_RECALL_API_KEY` env fallback.
- `store` module: SQLite open + WAL/foreign-keys pragmas + hand-rolled
  migration v1 (SPEC §3.2 schema). `Meeting` / `NewMeeting` /
  `MeetingFilters` / `MeetingStatus` types, plus CRUD + tag + session
  helpers. Slug uniqueness check.
- `recall` module: ported Recall.ai REST client from the legacy reference,
  swapped `anyhow` for our `Error` enum, added a `RecallApi` trait for
  mocking, plus `delete_bot` and `check` methods.
- `notes` module: Tera-templated prompt rendering + `claude --print`
  shell-out. Embeds `prompts/notes.md.tera`.
- `bundle` module: `meeting.toml` + `transcript.{jsonl,md}` + `notes.md`
  writer with both directory and tar-stream variants.
- `session` module: `SessionState` JSON shape for crash recovery (SPEC §8).
- `search` module: `LIKE`-based multi-token AND search across `title`,
  `notes_md`, `transcript_md`, with snippet extraction.
- `util` helpers: platform detection, slug + collision disambiguation,
  duration parsing.
- `secret` command: `set` reads stdin, `get` masks by default (`--reveal`
  to print), `delete` clears the keychain, `check` verifies the key
  against the Recall.ai API.
- `import` command: parses `.jsonl`, `.vtt`, `.srt`, `.txt` transcripts;
  derives speakers, renders `transcript_md` and `transcript_jsonl`,
  inserts a meeting row. `--notes` shells out to Claude; `--notes-file`
  uses provided markdown.
- `transcript` module: shared utterance type and the four parsers.
- `show` command: metadata table by default; `--transcript`, `--notes`,
  and `--json` flags honored.
- `list` command: filters `--since`, `--tag`, `--status`, `--limit`,
  `--all`. `cancelled` meetings are hidden by default.
- `export` command: write `<slug>/{meeting.toml, transcript.{jsonl,md},
  notes.md}` to a directory or stream a tar archive to stdout via
  `--out -`. `--force` overwrites; `--format jsonl|md|all` selects subsets.
- `search` command: snippet-style matches across `title`, `notes_md`,
  `transcript_md` with `--in`, `--context`, `--no-snippets`, plus the same
  filter flags as `list`.
- `notes` command: regenerate notes for a captured meeting; refuses to
  overwrite existing notes without `--force`.
- `fetch` command: import a transcript by Recall.ai bot id; refuses
  duplicates if the bot was already imported.
- `run` command: full SPEC §5.1 flow — dispatch a bot, poll status with a
  15s/5s/60s cadence, persist a `sessions.state_json` checkpoint each
  status transition, fetch transcript on `done`, optionally generate
  notes. Honors `--resume <id>`, `--dry-run`, and `--platform` override.
  SIGINT triggers a graceful cancel (DELETE the bot, mark cancelled,
  drop the session, exit 130); a second SIGINT exits immediately.
- `clean` command: drop session rows for terminal meetings + sessions
  stale >24h; cascade-delete meetings older than `--older-than`. `--dry-run`
  counts without deleting.
- Integration tests: `tests/migrations.rs`, `tests/import_export_roundtrip.rs`,
  `tests/search.rs`, plus a `MOOT_RECALL_INTEGRATION=1`-gated live
  Recall.ai auth check.
