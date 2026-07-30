# gcs

`gcs` is a small command-line tool for switching Git identity profiles in the
current repository. It changes only repository-local Git configuration.

See [CHANGELOG.md](CHANGELOG.md) for release history.

## Install

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/frankittee/Git-Config-Switch/main/install.sh | sh
```

The installer downloads the latest release for Linux or macOS, verifies its
SHA-256 checksum, and installs `gcs` to `~/.local/bin`. Set `GCS_INSTALL_DIR`
to use a different destination, or `GCS_VERSION` to install a specific version.

## Usage

Run `gcs` without a subcommand to open the interactive terminal interface:

```sh
gcs
```

The TUI shows saved profiles and the current Git identity side by side. Use
`↑`/`↓` or `j`/`k` to navigate, `Enter` or `u` to apply a profile, `a` to add,
`e` to edit, `d` to delete, and `q` to quit. Adding and editing use `Tab` to
move between fields, `Space` to toggle commit signing, `Enter` to save, and
`Esc` to cancel.

The existing subcommands remain available for scripts and direct operations.
Run `add` without field options to answer the prompts interactively:

```text
$ gcs add work
Git author name: Ada Lovelace
Git author email: ada@company.example
Enable commit signing? [y/N]: y
Signing key: ABC123
work
```

You can also provide all fields for scripts, or provide only some fields and
answer prompts for the missing values:

```sh
gcs add personal \
  --name "Ada Lovelace" \
  --email "ada@example.com"

gcs add work \
  --name "Ada Lovelace" \
  --email "ada@company.example" \
  --signing-key "ABC123"

gcs add another-work-account --name "Ada Lovelace"

gcs list
gcs show work
gcs edit work
gcs edit work --email "new-address@company.example"
gcs edit work --no-signing
gcs use work
gcs info
gcs remove personal
```

The TUI and interactive input require a terminal. In CI and other
non-interactive environments, use an explicit subcommand and provide both
`--name` and `--email` when adding a profile.

Profiles are saved in `$HOME/.config/git-config-switch/config.toml`. Setting
`GCS_CONFIG_DIR` overrides the containing directory, which is useful for isolated
automation and tests.

When a profile includes a signing key, `gcs use` sets `user.signingkey` and
enables `commit.gpgsign`. Switching to a profile without a signing key removes
both repository-local signing settings.

## Release

Pushing a version tag builds statically linked musl binaries for Linux x86_64
and Linux ARM64, plus native binaries for macOS Intel and Apple Silicon. The
artifacts are published in a GitHub Release, and the tag must match the version
in `Cargo.toml`.

```sh
# After updating the package version and committing it:
git tag v0.2.0
git push origin v0.2.0
```

Normal branch pushes do not trigger release builds.
