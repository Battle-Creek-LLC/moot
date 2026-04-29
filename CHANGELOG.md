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
