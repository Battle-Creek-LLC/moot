# Moot

Send a Recall.ai bot to a meeting, capture the transcript, generate notes. One binary, one SQLite file, no servers.

```
$ moot run --url https://meet.google.com/abc-defg-hij --notes
✓ Bot dispatched (abc-123)
✓ In call · 47m
✓ Transcript fetched · 412 utterances
✓ Notes generated
✓ Saved as weekly-staff-2026-04-29 (01HXYZ...)
```

## What it does

- Dispatches a [Recall.ai](https://recall.ai) bot to a Google Meet, Microsoft Teams, or Zoom URL.
- Polls until the meeting ends, pulls the transcript, stores it.
- Optionally generates structured notes by shelling out to the [Claude CLI](https://docs.claude.com/claude-code).
- Stores everything in a single SQLite database at an XDG-compliant path.
- Exports any meeting back to plain files (`meeting.toml` + `transcript.{jsonl,md}` + `notes.md`) for downstream agents.

## What it doesn't do

- Doesn't transcribe audio itself — Recall.ai does that.
- Doesn't run a server, daemon, or webhook listener (yet).
- Doesn't know about any external knowledge graph, issue tracker, or note system. If you want captured meetings to flow into something else, point an agent at `moot export` or query the SQLite directly.

## Install

```sh
cargo install moot
```

Pre-built binaries are published on the [releases page](https://github.com/Battle-Creek-LLC/moot/releases) for Linux (x86_64, aarch64) and macOS (x86_64, aarch64).

## Setup

```sh
# Stash your Recall.ai API key in the OS keychain.
moot secret set
# Paste key, press enter.

# Verify it works.
moot secret check
```

The key is stored via the OS keychain (macOS Keychain, Linux Secret Service). It's never written to disk and never appears in the SQLite database.

If you can't use a keychain (CI, headless server), set `MOOT_RECALL_API_KEY` instead.

## Usage

```sh
# Capture a live meeting.
moot run --url https://meet.google.com/... --notes

# Re-pull a transcript you already dispatched a bot for.
moot fetch --bot-id abc-123 --notes

# Import a transcript from somewhere else.
moot import -f standup.vtt --title "Standup" --participants "Alice,Bob"

# Browse what you've captured.
moot list --since 7d

# Search by content across title, notes, and transcripts.
moot search "decision about auth"

moot show weekly-staff-2026-04-29
moot show weekly-staff-2026-04-29 --transcript

# Hand a meeting off to an agent or downstream tool.
moot export weekly-staff-2026-04-29 --out /tmp/m

# Tidy up.
moot clean --sessions
moot clean --older-than 90d
```

Every command supports `--json` for machine-readable output.

## Where things live

| Path | Contents |
|---|---|
| `~/.local/share/moot/moot.db` (Linux) | SQLite database |
| `~/Library/Application Support/dev.battlecreek.moot/moot.db` (macOS) | SQLite database |
| `~/.config/moot/config.toml` (Linux) | Optional config |
| OS keychain, service `moot`, entry `recall-api-key` | API key |

Override the database path with `--db <path>` or `MOOT_DB`.

## Design

See [SPEC.md](./SPEC.md) for the full design — schema, CLI surface, Recall.ai integration details, error model, and roadmap.

## License

MIT. See [LICENSE](./LICENSE).
