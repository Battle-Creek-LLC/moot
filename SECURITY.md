# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities privately via GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
feature on this repository's **Security** tab.

We aim to acknowledge reports within 5 business days and provide an
initial assessment within 10 business days. Please do not file public
issues for security-sensitive reports.

## Supported Versions

This project follows a rolling-release model — only the latest tagged
release on `main` is supported.

## Threat model notes

- The Recall.ai API key is stored via the OS keychain (macOS Keychain,
  Linux Secret Service). It is never written to the SQLite database or
  logged. The `MOOT_RECALL_API_KEY` env-var fallback exists for
  headless installs.
- Captured transcripts and notes live in plain text in the local SQLite
  database. Encryption at rest is on the v0.2+ roadmap; for now, rely on
  full-disk encryption for sensitive meetings.
- `moot run` shells out to the `claude` CLI for notes generation. The
  prompt body is passed via stdin, not as command-line arguments, to
  avoid shell-escaping issues with adversarial transcripts.
