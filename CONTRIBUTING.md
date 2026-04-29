# Contributing

## Dev setup

Moot is a single-crate Rust project. You need a current stable Rust toolchain
(edition 2024 requires Rust 1.85+). On Debian/Ubuntu you also need
`libdbus-1-dev` and `pkg-config` for the keychain crate.

```sh
cargo build
cargo test
```

## Commits

Conventional commit subjects are recommended (`feat:`, `fix:`, `chore:`,
`docs:`, `refactor:`, `test:`). Not enforced via tooling — just follow the
style of recent history.

## Licensing

By contributing to Moot you agree that your contributions are licensed under
the MIT License (see `LICENSE`).
