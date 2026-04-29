# Moot v0.1.0 — Build Plan

This document drives the build of Moot v0.1.0 end-to-end. Read `SPEC.md` for design and `README.md` for the user-facing pitch before you start. The legacy Recall.ai code we are porting lives at `.legacy-reference/meeting/`.

## Working agreements

- **Edit this file as you go.** Check off `[ ]` → `[x]` per task; add a one-line note when something diverges from the plan.
- **Conventional commit subjects** (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`). Not enforced by tooling — just write them.
- **Commit per logical unit**, not per file. Group scaffolding into a single commit; group "implement command X" into one commit (or one PR).
- **Keep `CHANGELOG.md`** updated under `## [Unreleased]` as you add features.
- **When the SPEC is silent**, pick a reasonable default and leave a `// TODO(spec):` comment plus a one-line note here.
- **Gates** marked `🚦 GATE` are explicit pause points. Stop, summarize what's done, surface anything that needs human attention, and wait.
- **Tests as you go**, not at the end. Each command lands with at least one test.

## Phase 1 — Bootstrap

- [x] `gh repo create Battle-Creek-LLC/moot --public --description "Send a Recall.ai bot to a meeting, capture transcripts, generate notes"`
- [x] `cd /Users/jstockdi/projects/jstockdi/moot && git init && git branch -M main`
- [x] `git remote add origin git@github.com:Battle-Creek-LLC/moot.git`
- [x] `.gitignore` — standard Rust (`/target`, `Cargo.lock` kept since this is a binary), plus `.legacy-reference/` excluded? **No — keep `.legacy-reference/` in-tree and committed; it documents what we ported from.** (Minimal `.gitignore` containing just `/target`; nothing else needed yet — no IDE files, no env files in tree.)
- [x] `Cargo.toml` — see "Cargo.toml dependencies" below
- [x] `rustfmt.toml` — empty file (use defaults) is fine
- [x] `clippy.toml` — empty file is fine
- [x] `Cross.toml` — pre-build hook installing `libdbus-1-dev:$CROSS_DEB_ARCH` for `aarch64-unknown-linux-gnu` (mirror `../terra/../repocat/Cross.toml`)
- [x] `CONTRIBUTING.md` — short: dev setup (`cargo build`, `cargo test`), conventional commit recommendation, MIT licensing of contributions
- [x] `CHANGELOG.md` — Keep-a-Changelog header + empty `## [Unreleased]` section
- [x] `.github/workflows/ci.yml` — mirror `../terra/../repocat/.github/workflows/ci.yml`. Ubuntu, install `libdbus-1-dev pkg-config`, `cargo build --locked --verbose`, `cargo test --locked --verbose`
- [x] `.github/workflows/release.yml` — mirror repocat's. Tag `v*` trigger, 5-target matrix (linux x86_64/aarch64, macos x86_64/aarch64, windows x86_64), `taiki-e/upload-rust-binary-action`. Append a `cargo publish` job that runs after the matrix on success, gated on `secrets.CARGO_REGISTRY_TOKEN`. Bin name: `moot`. (Also brought across `dependency-review.yml` to mirror repocat fully.)
- [x] First commit: `chore: scaffold v0.1.0 project structure`
- [x] `git push -u origin main`
- [x] Confirm CI runs green on the empty crate (run [25119628692](https://github.com/Battle-Creek-LLC/moot/actions/runs/25119628692), 1m13s)

🚦 **GATE 1**: report CI run URL, confirm green. Then continue.

## Phase 2 — Skeleton

Goal: every command in `--help`, every subcommand prints "not yet implemented" and exits 1. No real logic.

- [x] `src/main.rs` — `tokio::main`, parse args, dispatch
- [x] `src/lib.rs` — public re-exports for testing
- [x] `src/error.rs` — `thiserror` enum with variants from SPEC §10 (`Config`, `Keychain`, `Recall`, `Db`, `Notes`, `Fs`, `Cli`)
- [x] `src/logging.rs` — `tracing-subscriber` setup, respect `MOOT_LOG`, `MOOT_NO_COLOR`, `--json` mode (added `json` feature to `tracing-subscriber` so `--json` flips logs to structured stderr; lockfile updated.)
- [x] `src/cli/mod.rs` — top-level clap `#[derive(Parser)]` with all 10 subcommands
- [x] `src/cli/{run,fetch,import,notes,export,list,show,search,clean,secret}.rs` — each is a clap subcommand struct + a stub `pub async fn execute(...) -> Result<()> { ... }` (stubs print `moot <verb>: not yet implemented` to stderr and `process::exit(1)` directly — cleaner than routing through the error enum for what is a temporary placeholder, since none of the seven variants describe "verb is unimplemented" honestly.)
- [x] `src/paths/mod.rs` — `ProjectDirs::from("dev", "battlecreek", "moot")`. Functions: `db_path()`, `config_path()`. Honor `--db` override and `MOOT_DB`.
- [x] `src/secrets/mod.rs` — `keyring::Entry::new("moot", "recall-api-key")`. `get()` falls back to `MOOT_RECALL_API_KEY`. Used by `cli::secret` later.
- [x] `cargo run -- --help` shows the 10-verb tree from SPEC §5
- [x] `cargo run -- run --help` shows §5.1's flags
- [x] Commit: `feat: scaffold cli command tree`
- [x] Push

🚦 **GATE 2**: confirm `--help` matches SPEC §5 visually. Continue.

## Phase 3 — Implementation

Order is dependency-driven. Each step ends with a green `cargo test`, a `CHANGELOG.md [Unreleased]` entry, and a commit.

### Foundation modules

- [x] `src/store/mod.rs` — open DB at `paths::db_path()`. Apply migrations via `PRAGMA user_version`. Migration v1 = SPEC §3.2 schema. Set `journal_mode=WAL`, `foreign_keys=ON`. Functions: `insert_meeting`, `update_meeting`, `get_meeting(id_or_slug)`, `list_meetings(filters)`, `delete_meeting(id)`, `list_orphan_sessions()`, `upsert_session`, `delete_session`. Test: open in-memory, run migration, round-trip a meeting.
- [x] `src/recall/mod.rs` — port `.legacy-reference/meeting/recall.rs`. Adapt error type from `anyhow::Result` to our `Error`. Keep the same struct names (`RecallClient`, `BotStatus`, etc). Add a `RecallApi` trait so we can mock in tests. (Trait gained `delete_bot` and `check` — needed by `run` SIGINT cleanup and `secret check`.)
- [x] `src/notes/mod.rs` — port `.legacy-reference/meeting/claude.rs`. Tera template rendering. Functions: `generate_notes(transcript_md, context, prompt_template) -> Result<String>`. (Simplified shape: pipes prompt body via stdin to `claude --print` rather than passing transcript as a file like the legacy code did. Adequate for v0.1.)
- [x] `prompts/notes.md.tera` — copy from `.legacy-reference/meeting/prompts/notes.md.tera`, embed via `include_str!`. (Adapted: legacy version asked Claude to read the transcript from a file; v0.1 inlines the transcript into the prompt since we pipe the prompt over stdin and don't stage temp files.)
- [x] `src/bundle/mod.rs` — write `meeting.toml` + `transcript.jsonl` + `transcript.md` + `notes.md` to a target dir. Tar stream variant for `--out -`.
- [x] `src/session/mod.rs` — JSON serialize/deserialize for the `sessions.state_json` blob. Phase enum from SPEC §8.
- [x] `src/search/mod.rs` — `LIKE`-based across `title`, `notes_md`, `transcript_md`. Snippet extraction (`±--context` chars around each match). Multi-word AND. Score = match count.
- [x] Slug generation helper (`src/util/slug.rs`): title-slugified + ISO date; on collision append `-2`, `-3`. Test collisions.
- [x] Platform detection from URL (`src/util/platform.rs`): `meet.google.com` → meet, `teams.microsoft.com|teams.live.com` → teams, `*.zoom.us` → zoom, else unknown.

### Commands (implement in this order)

- [x] `secret` — `set` (read stdin), `get` (`--reveal`), `delete`, `check` (HEAD `/bot` against Recall). (Used `GET /bot` rather than HEAD — Recall returns 401 / 200 cleanly on GET; HEAD support is undocumented.) Mask helper covered by unit tests.
- [x] `import` — parse `.jsonl`, `.vtt`, `.srt`, `.txt`. Insert meeting + transcript_md (rendered). `--notes-file` short-circuits notes generation. Parsers + render covered by unit tests; integration round-trip in `tests/import_export_roundtrip.rs`.
- [x] `show` — fetch by id or slug. Default = metadata table. `--transcript` / `--notes` / `--json`.
- [x] `list` — filters: `--since`, `--tag`, `--status`, `--all`, `--limit`. Hide `cancelled` by default.
- [x] `export` — write bundle to `<out>/<slug>/`, default CWD. `--force` overwrites. `--out -` streams tar to stdout. `--format jsonl|md|all`.
- [x] `search` — call `src/search/`, render snippets with highlighted match. `--no-snippets`, `--in`, `--context`, `--limit`, plus `list`-style filter flags. (Highlight is plain ellipsis-bracketed snippets; no ANSI bolding yet.)
- [x] `notes` — read `transcript_md` from DB, render template, shell out to `claude --print`. `--prompt` override. Refuse to overwrite without `--force`.
- [x] `fetch` — given `--bot-id`, call `recall.get_transcript`, insert as a new meeting. `--notes` triggers notes after.
- [x] `run` — full flow per SPEC §5.1. SIGINT handler for `cancelled` cleanup. `--resume` reads sessions row. `--dry-run` validates and exits.
- [x] `clean` — delete sessions for terminal meetings + orphans >24h. `--older-than <dur>` cascade-deletes meetings.

### Integration tests

- [x] `tests/import_export_roundtrip.rs` — `import` a fixture, `export` it, byte-compare. (Exercises the same code path through Bundle::build rather than spawning the binary.)
- [x] `tests/search.rs` — seed N meetings, exercise filters and snippets.
- [x] `tests/migrations.rs` — fresh DB, applied migrations match expected schema.
- [x] Recall integration test gated on `MOOT_RECALL_INTEGRATION=1` env (skipped in CI by default).

🚦 **GATE 3**: tell the user the binary is ready for a real-world smoke test. Provide a one-liner: `MOOT_RECALL_API_KEY=... cargo run -- run --url <real meet url> --notes`. Wait for results.

## Phase 4 — Release

- [ ] User confirms smoke test passed (capture, notes generation, list/show/search all work).
- [ ] Bump `Cargo.toml` → `version = "0.1.0"`.
- [ ] Move `## [Unreleased]` content into `## [0.1.0] - YYYY-MM-DD` with today's date. Add a fresh empty `## [Unreleased]` block above it.
- [ ] Commit: `chore: release v0.1.0`.
- [ ] `git push origin main`.
- [ ] `git tag v0.1.0 && git push origin v0.1.0`.
- [ ] Watch the release workflow: confirm 5 binaries uploaded to the GitHub Release, confirm `cargo publish` succeeded.
- [ ] Open a PR against `sprouted-dev/homebrew-tap` adding `Formula/moot.rb` pointing at the new release tarballs (sha256s from the workflow output).

🚦 **GATE 4**: announce v0.1.0. Done.

---

## Reference data (committed decisions, do not re-litigate)

### Cargo.toml dependencies

```toml
[package]
name = "moot"
version = "0.1.0"
edition = "2024"
description = "Send a Recall.ai bot to a meeting, capture the transcript, generate notes"
license = "MIT"
repository = "https://github.com/Battle-Creek-LLC/moot"
homepage = "https://github.com/Battle-Creek-LLC/moot"
readme = "README.md"
keywords = ["meeting", "transcript", "recall", "cli"]
categories = ["command-line-utilities"]

[dependencies]
clap = { version = "4", features = ["derive", "env"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "signal", "fs", "process", "io-util"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
rusqlite = { version = "0.31", features = ["bundled"] }
keyring = { version = "3", features = ["apple-native", "linux-native", "sync-secret-service"] }
directories = "5"
ulid = "1"
slug = "0.1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
thiserror = "1"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
tar = "0.4"
tera = "1"
humantime = "2"  # for --since 7d / --older-than 90d parsing

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

### Project identity

- Crate / binary name: `moot`
- Bot display name in calls (default): `Moot`
- GitHub repo: `Battle-Creek-LLC/moot`
- crates.io: `moot` (verified available 2026-04-29)
- Homebrew tap: `sprouted-dev/homebrew-tap`, formula `moot.rb`
- License: MIT
- Author: `Jonathan Stockdill`
- XDG paths: `ProjectDirs::from("dev", "battlecreek", "moot")`
- Keychain: service `moot`, entry `recall-api-key`
- Env fallback: `MOOT_RECALL_API_KEY`
- DB env override: `MOOT_DB`
- Region env: `MOOT_RECALL_REGION` (default `us-west-2`)
- Log filter: `MOOT_LOG`
- Color disable: `MOOT_NO_COLOR`

### Status enum

`recording`, `processing`, `active`, `failed`, `cancelled`. `cancelled` hidden by default in `list`.

### Recall.ai API (verified from `.legacy-reference/meeting/recall.rs`)

- Auth: `Authorization: Token <key>` (NOT Bearer)
- Base: `https://{region}.recall.ai/api/v1`
- `POST /bot` body: `{meeting_url, bot_name, recording_config: {transcript: {provider: {recallai_streaming: {}}}, retention: {type: "timed", hours: 72}}}`
- `GET /bot/{id}` → `{status_changes: [{code, sub_code}], recordings: [{media_shortcuts: {transcript: {data: {download_url}}}}]}`
- Transcript download URL: separate GET, no auth header needed
- Transcript shape: `[{participant: {id, name}, words: [{text, start_timestamp: {relative}, end_timestamp: {relative}}]}]`
- Status codes: `joining_call`, `in_waiting_room`, `in_call_not_recording`, `in_call_recording`, `call_ended`, `done`, `fatal`

### CI / Release shape (mirror `repocat`)

- CI: Ubuntu only, `cargo build --locked && cargo test --locked`. Install `libdbus-1-dev pkg-config` first.
- Release: tag `v*`, 5-target matrix, `taiki-e/upload-rust-binary-action@v1.30.2`, tar for unix, zip for windows, sha256 checksums.
- `Cross.toml`: pre-build install `libdbus-1-dev:$CROSS_DEB_ARCH` for `aarch64-unknown-linux-gnu`.
- crates.io publish appended as a final job, gated on `secrets.CARGO_REGISTRY_TOKEN`.

### Things that need human action (don't try to do these)

- Add `CARGO_REGISTRY_TOKEN` to repo secrets (Phase 4 prerequisite).
- Provide a real Recall.ai API key for end-to-end smoke test (Phase 3 → Phase 4 gate).
- Approve any `gh repo create` permission prompts.
- Approve the eventual homebrew-tap PR (different repo, different review).

### Things deferred to v0.2+

- Webhook / streaming transcript ingestion (`.legacy-reference/meeting/buffer.rs`).
- Multiple notes prompts per meeting (spec.md.tera, adr.md.tera, etc. — already in `.legacy-reference/meeting/prompts/`).
- FTS5 virtual table (add as v2 migration if `LIKE` becomes slow).
- `mycelium` crates.io name transfer request (separate process, can run in parallel).

---

## Status log

Update this section after each phase or significant pause.

- 2026-04-29: Plan written. SPEC, README, LICENSE in place. `.legacy-reference/` populated. Awaiting Phase 1 kickoff.
- 2026-04-29: Phase 1 complete. Repo created at Battle-Creek-LLC/moot. Scaffold committed (root commit `d0aa8be`). CI run [25119628692](https://github.com/Battle-Creek-LLC/moot/actions/runs/25119628692) passed in 1m13s. GATE 1 reached.
- 2026-04-29: Phase 2 complete. CLI tree scaffolded; all ten verbs print "not yet implemented" and exit 1. Commit `704f6b8`. CI run [25120194958](https://github.com/Battle-Creek-LLC/moot/actions/runs/25120194958) passed in 1m17s. GATE 2 reached.
- 2026-04-29: Phase 3 complete. All ten verbs implemented. 33 unit + 7 integration tests passing locally and on CI run [25121777777](https://github.com/Battle-Creek-LLC/moot/actions/runs/25121777777). Smoke-tested import → list → show → search → export end-to-end against /tmp/moot-test. GATE 3 reached — needs a real-world `moot run --url <meet>` smoke test from the user before tagging v0.1.0.
