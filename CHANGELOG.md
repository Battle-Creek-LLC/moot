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
