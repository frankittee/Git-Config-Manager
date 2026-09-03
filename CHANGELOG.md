# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- - -
## [v0.6.0](https://github.com/frankittee/Git-Config-Manager/compare/81965f560517eec28d5f5468a3bdd9412db587ac..v0.6.0) - 2026-09-03
#### Features
- migrate CLI to usage-rs - ([f534c91](https://github.com/frankittee/Git-Config-Manager/commit/f534c916443c53fb6e2da4e7c04f6695c35bafd4)) - frankittee
#### Documentation
- update repository links - ([041809e](https://github.com/frankittee/Git-Config-Manager/commit/041809edf746a0b4d6dcbb0522fc1db2f546a791)) - frankittee
#### Miscellaneous Chores
- remove local GitHub token - ([81965f5](https://github.com/frankittee/Git-Config-Manager/commit/81965f560517eec28d5f5468a3bdd9412db587ac)) - frankittee

- - -


## [0.5.0] - 2026-08-15

### Changed

- Renamed the command-line executable to `g`.

## [0.4.4] - 2026-08-14

### Fixed

- SSH signing profiles now set `gpg.format=ssh` when applied.

## [0.4.3] - 2026-08-03

### Fixed

- Profile updates now preserve a symlinked `config.toml` and atomically update its target file.

## [0.4.2] - 2026-08-01

### Fixed

- The SSH host alias picker now fits its visible entries and shows an accurate scrollbar when more aliases are available.

## [0.4.1] - 2026-08-01

### Fixed

- SSH host alias discovery now uses `ssh2-config-rs` to follow nested `Include` directives, including relative paths, `~`, and glob patterns.

## [0.4.0] - 2026-07-31

### Added

- Profiles can now store an optional SSH host alias. Applying one rewrites all SSH remote fetch and explicit push URLs after validating a literal matching `Host` entry in `~/.ssh/config`.

## [0.3.0] - 2026-07-30

### Added

- Added a Ratatui terminal interface for browsing, creating, editing, deleting, and applying profiles, with responsive layouts and contextual status information. Run `g` without a subcommand to open it.
- Added an installer for verified Linux and macOS release binaries.

## [0.2.0] - 2026-07-30

### Changed

- `g use` now applies a profile to the global Git configuration when run directly from the user's home directory and reports the written configuration file after success.
- Renamed `g current` to `g info`.

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

[Unreleased]: https://github.com/frankittee/Git-Config-Manager/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/frankittee/Git-Config-Manager/compare/v0.4.4...v0.5.0
[0.4.4]: https://github.com/frankittee/Git-Config-Manager/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/frankittee/Git-Config-Manager/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/frankittee/Git-Config-Manager/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/frankittee/Git-Config-Manager/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/frankittee/Git-Config-Manager/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/frankittee/Git-Config-Manager/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/frankittee/Git-Config-Manager/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/frankittee/Git-Config-Manager/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/frankittee/Git-Config-Manager/releases/tag/v0.1.0
