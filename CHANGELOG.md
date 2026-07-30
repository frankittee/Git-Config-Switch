# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-07-30

### Fixed

- Replaced the platform-dependent `file` output check with an ELF `INTERP`
  segment check for static Linux musl binaries.
- Fixed the x86_64 Linux musl release job.

## [0.1.0] - 2026-07-30

### Added

- Added interactive Git identity profile creation and editing.
- Added profile listing, inspection, removal, application, and current-profile
  detection.
- Added optional Git commit signing configuration.
- Added atomic profile storage at
  `$HOME/.config/git-config-switch/config.toml`.
- Added transactional repository-local Git configuration updates with rollback.
- Added tagged GitHub releases for Linux x86_64/ARM64 musl and macOS
  Intel/Apple Silicon.

[Unreleased]: https://github.com/frankittee/Git-Config-Switch/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/frankittee/Git-Config-Switch/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/frankittee/Git-Config-Switch/releases/tag/v0.1.0
