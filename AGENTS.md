# Repository Guidelines

## Project Structure & Module Organization

This repository contains the Rust CLI binary `g`.

- `src/main.rs` parses commands and coordinates profile and Git operations.
- `src/cli.rs` defines the `usage-rs` command-line interface.
- `src/profiles.rs` validates and atomically persists profiles.
- `src/git.rs` reads and updates repository-local Git configuration.
- `src/input.rs` implements line-based interactive prompts used when no terminal is available.
- `src/prompt.rs` implements the interactive `add` and `edit` prompts with [demand](https://github.com/jdx/demand); its validators and alias suggestions are pure and unit-tested.
- `skills/g/SKILL.md` is the agent skill that documents safe `g` usage; keep it in sync with the CLI.
- `.github/workflows/ci.yml` runs formatting, Clippy, and tests on Linux and macOS.
- `tests/cli.rs` contains end-to-end CLI tests using isolated temporary directories and Git repositories.
- `.github/workflows/release.yml` builds tagged releases for Linux musl and macOS targets.
- `hk.pkl` configures Git hooks; `cliff.toml` configures git-cliff changelog generation; `scripts/release.sh` (run via `mise run release`) creates release commits and tags.

Profiles default to `$HOME/.config/git-config-switch/config.toml`. Tests must set `G_CONFIG_DIR` and must not modify a developer's real configuration.

## Build, Test, and Development Commands

Use the stable Rust toolchain through mise. Wrap all Rust-related commands with `mise x --`:

```sh
mise x -- cargo build                         # Compile a debug binary
mise x -- cargo run -- add work               # Run the CLI locally
mise x -- cargo test                          # Run unit and integration tests
mise x -- cargo fmt --check                   # Verify rustfmt formatting
mise x -- cargo clippy --all-targets -- -D warnings
mise x -- cargo build --release --locked      # Build an optimized, locked release
```

Run formatting with `mise x -- cargo fmt` before committing. Never edit `Cargo.lock` manually except for an intentional package-version change.

Git hooks are managed by [hk](https://hk.jdx.dev/) and configured in `hk.pkl`. Install them once after cloning:

```sh
mise install
mise x -- hk install --mise
```

The `commit-msg` hook rejects messages that are not Conventional Commits, `pre-commit` runs `cargo fmt` on staged Rust files, and `pre-push` runs `cargo fmt --check` plus Clippy. Run the same checks manually with `mise run check`, or apply fixes with `mise run fix`. Version bumps follow Conventional Commits: `fix` creates a patch release, `feat` a minor release, and a breaking change a major release.

## Coding Style & Naming Conventions

Follow standard `rustfmt` output with four-space indentation. Use `snake_case` for modules, functions, variables, and test names; use `PascalCase` for structs and enums. Keep functions small, behavior explicit, and error messages actionable. Prefer `anyhow::Context` at I/O and process boundaries. Avoid unnecessary abstractions or new dependencies for small features.

Keep each Markdown paragraph and list item on a single line. Do not hard-wrap prose across multiple lines; preserve line breaks only where Markdown structure or code requires them.

## Testing Guidelines

Place focused unit tests beside their module under `#[cfg(test)]`; add observable CLI behavior to `tests/cli.rs`. Name tests after behavior, such as `duplicate_suggests_edit`. Use `tempfile` for isolation and invoke Git through `std::process::Command`. Cover success paths, validation failures, non-terminal input, signing transitions, and rollback-sensitive changes. All tests, Clippy, and formatting checks must pass.

## Commit & Pull Request Guidelines

Use concise Conventional Commit-style subjects matching repository history, for example `feat: add profile import` or `fix: preserve signing config`. Keep commits scoped and update `CHANGELOG.md` for user-visible changes.

Pull requests should explain the behavior change, motivation, CLI examples, and validation performed. Link relevant issues. Screenshots are only needed for terminal output whose presentation changed. Do not combine unrelated refactors with feature or bug-fix work.

Merge pull requests with squash merge by default so all changes from a PR become a single commit on the target branch. Keep version and release changes in a separate release commit.

Run GitHub CLI commands through mise, for example `mise x -- gh auth status`, rather than invoking `gh` directly.

## Demo Recording

`demo/demo.tape` is a [VHS](https://github.com/charmbracelet/vhs) script that records `demo/demo.gif` for the README. Regenerate it with `mise run demo` after user-visible CLI or TUI changes. VHS needs `ttyd` on `PATH` (for example `brew install ttyd`); the task fetches `vhs` and `ffmpeg` through mise. The tape runs with a throwaway `HOME` (containing a fake `~/.ssh/config` with `github-personal` and `github-work`), `G_CONFIG_DIR`, and `/tmp/my-project`, so it never touches or reveals real profiles or SSH hosts. It waits for the prompts and TUI to render before typing, because keystrokes sent before raw mode is enabled are echoed by the shell instead.

## Release Safety

Releases are triggered only by `v*` tags. Run `mise run release` to calculate the next version with git-cliff, run fmt/Clippy/tests, update `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`, create the `chore(version): vX.Y.Z` release commit, and tag it. Pass an explicit version with `mise run release 1.2.3` when needed. The script only prepends the new section to `CHANGELOG.md`, so never rewrite older entries. Then run `git push --follow-tags`. Ensure the tag exactly matches the package version before pushing.
