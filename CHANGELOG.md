# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- SSH host alias discovery now uses `ssh2-config-rs` to follow nested `Include` directives, including relative paths, `~`, and glob patterns.

## [0.4.0] - 2026-07-31

### Added

- Profiles can now store an optional SSH host alias. Applying one rewrites all SSH remote fetch and explicit push URLs after validating a literal matching `Host` entry in `~/.ssh/config`.

## [0.3.0] - 2026-07-30

### Added

- Added a Ratatui terminal interface for browsing, creating, editing, deleting, and applying profiles, with responsive layouts and contextual status information. Run `gcs` without a subcommand to open it.
- Added an installer for verified Linux and macOS release binaries.

## [0.2.0] - 2026-07-30

### Changed

- `gcs use` now applies a profile to the global Git configuration when run directly from the user's home directory and reports the written configuration file after success.
- Renamed `gcs current` to `gcs info`.

## [0.1.1] - 2026-07-30

### Fixed

- Replaced the platform-dependent `file` output check with an ELF `INTERP` segment check for static Linux musl binaries.
- Fixed the x86_64 Linux musl release job.

## [0.1.0] - 2026-07-30

### Added

- Added interactive Git identity profile creation and editing.
- Added profile listing, inspection, removal, application, and current-profile detection.
- Added optional Git commit signing configuration.
- Added atomic profile storage at `$HOME/.config/git-config-switch/config.toml`.
- Added transactional repository-local Git configuration updates with rollback.
- Added tagged GitHub releases for Linux x86_64/ARM64 musl and macOS Intel/Apple Silicon.

[Unreleased]: https://github.com/frankittee/Git-Config-Switch/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/frankittee/Git-Config-Switch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/frankittee/Git-Config-Switch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/frankittee/Git-Config-Switch/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/frankittee/Git-Config-Switch/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/frankittee/Git-Config-Switch/releases/tag/v0.1.0
