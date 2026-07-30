# Repository Guidelines

## Project Structure & Module Organization

This repository contains the Rust CLI binary `gcs`.

- `src/main.rs` parses commands and coordinates profile and Git operations.
- `src/cli.rs` defines the `clap` command-line interface.
- `src/profiles.rs` validates and atomically persists profiles.
- `src/git.rs` reads and updates repository-local Git configuration.
- `src/input.rs` implements interactive prompts.
- `tests/cli.rs` contains end-to-end CLI tests using isolated temporary
  directories and Git repositories.
- `.github/workflows/release.yml` builds tagged releases for Linux musl and
  macOS targets.

Profiles default to `$HOME/.config/git-config-switch/config.toml`. Tests must
set `GCS_CONFIG_DIR` and must not modify a developer's real configuration.

## Build, Test, and Development Commands

Use the stable Rust toolchain:

```sh
cargo build                         # Compile a debug binary
cargo run -- add work               # Run the CLI locally
cargo test                          # Run unit and integration tests
cargo fmt --check                   # Verify rustfmt formatting
cargo clippy --all-targets -- -D warnings
cargo build --release --locked      # Build an optimized, locked release
```

Run formatting with `cargo fmt` before committing. Never edit `Cargo.lock`
manually except for an intentional package-version change.

## Coding Style & Naming Conventions

Follow standard `rustfmt` output with four-space indentation. Use `snake_case`
for modules, functions, variables, and test names; use `PascalCase` for structs
and enums. Keep functions small, behavior explicit, and error messages
actionable. Prefer `anyhow::Context` at I/O and process boundaries. Avoid
unnecessary abstractions or new dependencies for small features.

## Testing Guidelines

Place focused unit tests beside their module under `#[cfg(test)]`; add observable
CLI behavior to `tests/cli.rs`. Name tests after behavior, such as
`duplicate_suggests_edit`. Use `tempfile` for isolation and invoke Git through
`std::process::Command`. Cover success paths, validation failures, non-terminal
input, signing transitions, and rollback-sensitive changes. All tests, Clippy,
and formatting checks must pass.

## Commit & Pull Request Guidelines

Use concise Conventional Commit-style subjects matching repository history,
for example `feat: add profile import` or `fix: preserve signing config`.
Keep commits scoped and update `CHANGELOG.md` for user-visible changes.

Pull requests should explain the behavior change, motivation, CLI examples, and
validation performed. Link relevant issues. Screenshots are only needed for
terminal output whose presentation changed. Do not combine unrelated refactors
with feature or bug-fix work.

## Release Safety

Releases are triggered only by `v*` tags. Update both `Cargo.toml` and
`Cargo.lock`, commit the version change, and ensure the tag exactly matches it
before pushing.
